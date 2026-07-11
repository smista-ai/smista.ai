//! The resolver: derives the deterministic routing inputs for a turn.
//!
//! Routing in smista.ai is staged. The
//! [`TaskNormalizer`] classifies the task intent
//! without an LLM and extracts the relevant skills and touched files into a
//! [`NormalizedTask`](normalizer::NormalizedTask). The
//! [`PolicyMatcher`] evaluates the user's routing
//! rules against that task and picks exactly one route (an override, a matched
//! rule, or the default route) as a [`RouteMatch`](policy_matcher::RouteMatch).
//! The [`ContextSelector`] then selects the minimum
//! context the turn needs. Each stage is pure and free of side effects; the
//! execution orchestrator threads them together and turns the chosen route into
//! a usable model.
//!
//! [`Resolver::resolve`] is that single deterministic entry point. It runs the
//! five stages in order and returns the turn's [`ResolvedTurn`] plan: the chosen
//! model with its remaining fallbacks and the finalized context. It reads no
//! files, no database and makes no network call — every input arrives in
//! [`ResolveArgs`], already recalled and already decrypted by the caller (the
//! run state machine). Because the same call serves the API and the state
//! machine alike, the resolver owns its own input and output vocabulary
//! ([`TaskInput`], [`Workspace`], [`Attachments`], [`ResolvedTurn`]) rather than
//! the wire contract in [`smista_core::api`]; the caller maps between the two.
//!
//! The five stages run in this order:
//!
//! 1. normalize: the request becomes a [`NormalizedTask`](normalizer::NormalizedTask).
//! 2. match: the task and policy pick one [`RouteMatch`](policy_matcher::RouteMatch).
//! 3. context build: the route, recall and request fold into a
//!    [`CandidateSet`](context::CandidateSet), marked restricted-for-remote,
//!    required-or-discardable and sized.
//! 4. model select: the route and the candidate marks choose a usable model.
//! 5. context finalize: the candidate set is trimmed to the chosen model's window.
//!
//! Context selection straddles model selection: the candidate marks (privacy and
//! size) constrain the model choice, and the model's window then bounds the
//! finalized context.

pub(crate) mod context;
mod model;
mod normalizer;
pub(crate) mod policy_matcher;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use smista_core::api::ApiErrorCode;
use smista_core::intent::TaskIntent;
use smista_core::model::{ModelDescriptor, ModelReference, Provider, RoutingRequirements};
use smista_core::policy::{
    Classification, ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig,
};
use smista_core::routing::RoutingDecision;
use smista_core::skill::Skill;

use crate::router::resolver::context::{
    ContextError, ContextSelector, RecalledContext, ResolvedContext,
};
use crate::router::resolver::model::{ModelSelector, ModelSelectorError};
use crate::router::resolver::normalizer::TaskNormalizer;
use crate::router::resolver::policy_matcher::{PolicyMatchError, PolicyMatcher};

/// The user's request for one turn: prompt text, optional command and override.
///
/// The resolver's own form of the request, distinct from the wire
/// [`TaskInput`](smista_core::api::TaskInput): the caller builds it from the API
/// payload, or from internal state when the state machine drives a turn itself.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskInput {
    /// The prompt text.
    pub text: String,
    /// Explicit task command, overriding intent detection when set.
    pub command: Option<TaskIntent>,
    /// Explicit model to use, bypassing routing when set.
    pub explicit_model: Option<ModelReference>,
}

/// Snapshot of the workspace a turn runs against.
///
/// The resolver's own form of the workspace, distinct from the wire
/// [`Workspace`](smista_core::api::Workspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Absolute path to the workspace root.
    pub root: PathBuf,
    /// Current git branch, if the workspace is a git repository.
    pub git_branch: Option<String>,
    /// Current git diff, if any.
    pub git_diff: Option<String>,
    /// Paths the user explicitly referenced.
    pub referenced_paths: Vec<PathBuf>,
    /// The file the user is actively editing, if any.
    pub active_file: Option<PathBuf>,
}

/// Local content the caller supplies because the resolver has no filesystem.
///
/// The resolver's own form of the attachments, distinct from the wire
/// [`Attachments`](smista_core::api::Attachments). History, memory and the
/// assembled context are not part of this — they arrive in
/// [`RecalledContext`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachments {
    /// Explicit `@path` files included with their content.
    pub files: Vec<ContextFile>,
    /// Instruction documents the caller read from disk.
    pub instructions: Vec<ContextInstruction>,
    /// Skills the user explicitly invoked, with their full `SKILL.md` body.
    ///
    /// Authoritative: routing rules may match on them by name, and their
    /// instructions are added to the model preamble.
    pub invoked_skills: Vec<Skill>,
    /// Skills offered for the serving model to activate by reading them.
    ///
    /// Same shape as the invoked skills, but distinct by role: they never
    /// influence routing, and the model decides whether to apply one.
    pub available_skills: Vec<Skill>,
}

/// A file the caller attached, with whether it is required.
///
/// The resolver's own form, distinct from the wire
/// [`ContextFile`](smista_core::api::ContextFile): the content hash the wire
/// type carries is a transport concern the resolver does not need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    /// Path of the file.
    pub path: PathBuf,
    /// File content.
    pub content: String,
    /// Whether the file is required (referenced or route-driving) and so cannot
    /// be dropped, as opposed to discardable supplementary context.
    pub required: bool,
}

/// An instruction document the caller attached as context.
///
/// The resolver's own form, distinct from the wire
/// [`ContextInstruction`](smista_core::api::ContextInstruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextInstruction {
    /// Where the instruction came from, for example `AGENTS.md`.
    pub source: String,
    /// The instruction content.
    pub content: String,
}

/// Every input one [`Resolver::resolve`] call needs, supplied by the caller.
///
/// The resolver reads nothing for itself: the state machine recalls history and
/// memory, decrypts them, and hands over the whole turn here. Borrowed where the
/// resolver only reads, so the caller keeps ownership of the large policy and
/// catalog values across the call.
#[derive(Debug)]
pub struct ResolveArgs<'a> {
    /// The user's request.
    pub input: &'a TaskInput,
    /// Snapshot of the workspace the turn runs against.
    pub workspace: &'a Workspace,
    /// Local files, instructions and skills the caller read from disk.
    pub attachments: &'a Attachments,
    /// History and memory the caller recalled and decrypted for the turn.
    pub recalled: &'a RecalledContext,
    /// Intent-classification rules and the default intent.
    pub classification: &'a ClassificationConfig,
    /// Routing rules and the default route.
    pub routing: &'a RoutingPolicy,
    /// Privacy constraints on the context sent to models.
    pub privacy: &'a PrivacyPolicy,
    /// The capabilities the task needs of a model. Its `estimated_tokens` is
    /// ignored: the resolver sets it from the required context it builds.
    pub requirements: RoutingRequirements,
    /// The router's known model facts, keyed by reference.
    pub catalog: &'a HashMap<ModelReference, ModelDescriptor>,
    /// The set of providers a credential was supplied for.
    pub credentialed: &'a HashSet<Provider>,
    /// Whether the caller restricts the turn to local models, regardless of the
    /// route — the request's `local_only` or `no_network` preference.
    pub local_only: bool,
}

/// The deterministic plan for one turn, the resolver's output.
///
/// Carries everything the run loop needs to serve the turn and everything the
/// [`Tracer`](crate::trace::Tracer) needs to record it: the
/// [`classification`](Self::classification) (the trace's classification event),
/// the [`routing`](Self::routing) decision (the trace's routing event), the
/// chosen [`model`](Self::model) with its remaining
/// [`fallbacks`](Self::fallbacks), and the finalized [`context`](Self::context),
/// whose references are the trace's context-selection events.
#[derive(Debug)]
pub struct ResolvedTurn {
    /// The deterministic classification: intent plus provenance.
    pub classification: Classification,
    /// The routing decision for the turn.
    pub routing: RoutingDecision,
    /// The model the turn should be served by.
    pub model: ModelDescriptor,
    /// The route's fallback chain after the chosen model, in order, for the run
    /// loop to walk if the live call fails.
    pub fallbacks: Vec<ModelReference>,
    /// The finalized context: what was included, what was excluded, and why.
    pub context: ResolvedContext,
    /// The tool permissions the matched route requires, as declared by the
    /// matched rule's `required_permissions`. Empty for an override or the
    /// default route, neither of which declares any.
    pub required_permissions: ToolsConfig,
}

/// An error resolving a turn.
///
/// One variant per fallible stage. A successful resolve produces a
/// [`ResolvedTurn`]; any stage that cannot decide fails the whole turn loud,
/// since routing never guesses.
#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    /// Context selection could not fit the required context to the chosen model.
    #[error("context error: {0}")]
    Context(#[from] ContextError),
    /// Model selection found no usable model for the route.
    #[error("model selection error: {0}")]
    Model(#[from] ModelSelectorError),
    /// Policy matching found no route for the task.
    #[error("routing error: {0}")]
    Routing(#[from] PolicyMatchError),
}

impl ResolverError {
    /// Maps the error onto the stable [`ApiErrorCode`] clients match on.
    ///
    /// A failed context fit is a context-window rejection; a model-selection
    /// failure is either an exhausted route or a forbidden override; an
    /// unmatched policy with no default is a missing route.
    pub fn api_code(&self) -> ApiErrorCode {
        match self {
            Self::Context(ContextError::WindowExceeded { .. }) => {
                ApiErrorCode::ContextWindowExceeded
            }
            Self::Model(
                ModelSelectorError::FallbackExhausted | ModelSelectorError::NoLocalModel,
            ) => ApiErrorCode::FallbackExhausted,
            Self::Model(ModelSelectorError::OverrideNotAllowed(_)) => {
                ApiErrorCode::OverrideNotAllowed
            }
            Self::Routing(PolicyMatchError::NoRoute) => ApiErrorCode::NoRoute,
        }
    }
}

/// The deterministic, LLM-free resolver: one entry point over the five stages.
///
/// Holds the four stage handlers, each a stateless unit struct. Every decision
/// is a pure function of [`ResolveArgs`], so the same inputs and the same policy
/// always produce the same [`ResolvedTurn`].
#[derive(Default, Debug)]
pub struct Resolver {
    context_selector: ContextSelector,
    model_selector: ModelSelector,
    normalizer: TaskNormalizer,
    policy_matcher: PolicyMatcher,
}

impl Resolver {
    /// Resolves one turn into its deterministic [`ResolvedTurn`] plan.
    ///
    /// Runs the five stages in order: normalize the task, match the routing
    /// policy, build the context candidate set, select a usable model under the
    /// candidate marks, then finalize the context to that model's window.
    /// Context selection straddles model selection, so its privacy and size
    /// marks constrain the model choice.
    ///
    /// The turn is restricted to local models when any candidate is restricted
    /// from remote providers or when the caller set
    /// [`local_only`](ResolveArgs::local_only). The required context sizes the
    /// `estimated_tokens` model selection fits against.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverError::Routing`] when no route matches and the policy
    /// has no default, [`ResolverError::Model`] when no usable model can serve
    /// the route, and [`ResolverError::Context`] when the required context does
    /// not fit the chosen model.
    pub fn resolve(&self, args: ResolveArgs<'_>) -> Result<ResolvedTurn, ResolverError> {
        tracing::trace!("stage 1/5: normalizing the task");
        // 1. Normalize the request into a classified, signal-bearing task.
        let task = self
            .normalizer
            .normalize(args.input, args.workspace, args.classification);

        tracing::trace!(intent = %task.intent(), "stage 2/5: matching the routing policy");
        // 2. Match the routing policy to pick exactly one route.
        let route = self.policy_matcher.match_policy(
            &task,
            args.routing,
            args.input.explicit_model.as_ref(),
        )?;

        tracing::trace!(model = %route.model(), "stage 3/5: building the context candidate set");
        // 3. Build the candidate set; its marks constrain model selection.
        let candidates = self.context_selector.build_candidates(
            &route,
            args.recalled,
            args.attachments,
            args.workspace,
            args.privacy,
        );
        let restricted = candidates.has_restricted_required_content() || args.local_only;
        let mut requirements = args.requirements;
        requirements.estimated_tokens = Some(candidates.required_tokens());

        tracing::trace!(
            restricted,
            required_tokens = candidates.required_tokens(),
            "stage 4/5: selecting a usable model"
        );
        // 4. Select a usable model under the route and the candidate marks.
        let selection = self.model_selector.select(
            &route,
            &requirements,
            args.catalog,
            args.credentialed,
            restricted,
        )?;

        // The routing record reads the chosen model before it is moved into the
        // plan; the remaining fallbacks are the chain after it.
        let routing = RoutingDecision {
            intent: route.intent,
            provider: selection.model.provider.clone(),
            model: selection.model.model.clone(),
            matched_rule: route.matched_rule().map(|rule| rule.name.clone()),
            fallback_used: selection.fallback_used,
            override_used: route.override_used(),
            reason: route.reason.clone(),
        };
        let fallbacks = remaining_fallbacks(&route, &selection.model.reference());
        // The route's required permissions are declared on the matched rule; an
        // override or the default route declares none.
        let required_permissions = route
            .matched_rule()
            .map(|rule| rule.required_permissions.clone())
            .unwrap_or_default();

        tracing::trace!(
            model = %routing.model,
            "stage 5/5: finalizing the context to the model window"
        );
        // 5. Finalize the context to the chosen model's window.
        let context = self
            .context_selector
            .finalize(candidates, &selection.model)?;

        tracing::debug!(
            intent = %routing.intent,
            provider = %routing.provider,
            model = %routing.model,
            fallback_used = routing.fallback_used,
            override_used = routing.override_used,
            "resolved turn"
        );
        Ok(ResolvedTurn {
            classification: task.classification,
            routing,
            model: selection.model,
            fallbacks,
            context,
            required_permissions,
        })
    }
}

/// The route's fallback references after the chosen model, in order.
///
/// The candidate chain is the primary model followed by the route's fallbacks;
/// the chosen model is the first usable one. The remaining fallbacks are those
/// after it — the alternatives the run loop has not yet tried. When the chosen
/// model is not on the route's chain (an override resolves to its own model),
/// there are none.
fn remaining_fallbacks(
    route: &policy_matcher::RouteMatch<'_>,
    chosen: &ModelReference,
) -> Vec<ModelReference> {
    let chain = std::iter::once(route.model()).chain(route.fallbacks());
    chain
        .skip_while(|reference| *reference != chosen)
        .skip(1)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use smista_core::model::{ModelAuthRequirement, ModelCapabilities, ModelParameters};

    use super::context::{CandidateKind, RecalledContext};
    use super::*;

    fn reference(value: &str) -> ModelReference {
        value.parse().expect("valid model reference")
    }

    /// A descriptor for `value`, with the given locality, key requirement and
    /// context window. Capabilities are the default (none required).
    fn descriptor(value: &str, local: bool, requires_key: bool, window: u32) -> ModelDescriptor {
        descriptor_with_reply(value, local, requires_key, window, None)
    }

    /// A descriptor with an explicit `max_output_tokens` reply reserve, so window
    /// tests can reason about the effective budget.
    fn descriptor_with_reply(
        value: &str,
        local: bool,
        requires_key: bool,
        window: u32,
        reply: Option<u32>,
    ) -> ModelDescriptor {
        let reference = reference(value);
        ModelDescriptor {
            provider: reference.provider,
            model: reference.model,
            display_name: None,
            local,
            auth: if requires_key {
                ModelAuthRequirement::ApiKey
            } else {
                ModelAuthRequirement::None
            },
            capabilities: ModelCapabilities::default(),
            max_context_tokens: window,
            max_output_tokens: reply,
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    fn catalog(descriptors: Vec<ModelDescriptor>) -> HashMap<ModelReference, ModelDescriptor> {
        descriptors
            .into_iter()
            .map(|descriptor| (descriptor.reference(), descriptor))
            .collect()
    }

    fn credentialed(providers: &[&str]) -> HashSet<Provider> {
        providers
            .iter()
            .map(|provider| provider.parse().expect("valid provider"))
            .collect()
    }

    fn policy(value: serde_json::Value) -> RoutingPolicy {
        serde_json::from_value(value).expect("valid routing policy")
    }

    fn privacy(restricted: &[&str]) -> PrivacyPolicy {
        serde_json::from_value(json!({ "restricted_paths": restricted })).expect("valid privacy")
    }

    fn workspace() -> Workspace {
        Workspace {
            root: PathBuf::from("/repo"),
            git_branch: None,
            git_diff: None,
            referenced_paths: Vec::new(),
            active_file: None,
        }
    }

    fn input(text: &str) -> TaskInput {
        TaskInput {
            text: text.to_string(),
            command: None,
            explicit_model: None,
        }
    }

    fn file(path: &str, content: &str, required: bool) -> ContextFile {
        ContextFile {
            path: PathBuf::from(path),
            content: content.to_string(),
            required,
        }
    }

    fn attachments(files: Vec<ContextFile>) -> Attachments {
        Attachments {
            files,
            instructions: Vec::new(),
            invoked_skills: Vec::new(),
            available_skills: Vec::new(),
        }
    }

    fn recalled() -> RecalledContext {
        RecalledContext {
            messages: Vec::new(),
            user_memory: Vec::new(),
            context_memory: Vec::new(),
        }
    }

    /// The owned inputs a resolve call borrows, so a test can build
    /// [`ResolveArgs`] against values that outlive the call.
    struct Inputs {
        input: TaskInput,
        workspace: Workspace,
        attachments: Attachments,
        recalled: RecalledContext,
        classification: ClassificationConfig,
        routing: RoutingPolicy,
        privacy: PrivacyPolicy,
        catalog: HashMap<ModelReference, ModelDescriptor>,
        credentialed: HashSet<Provider>,
        local_only: bool,
    }

    impl Inputs {
        fn args(&self) -> ResolveArgs<'_> {
            ResolveArgs {
                input: &self.input,
                workspace: &self.workspace,
                attachments: &self.attachments,
                recalled: &self.recalled,
                classification: &self.classification,
                routing: &self.routing,
                privacy: &self.privacy,
                requirements: RoutingRequirements::default(),
                catalog: &self.catalog,
                credentialed: &self.credentialed,
                local_only: self.local_only,
            }
        }
    }

    #[test]
    fn should_resolve_a_default_route_to_its_primary_model() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(vec![file("src/main.rs", "fn main() {}", true)]),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini", "fallbacks": ["ollama/llama3"] },
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![
                descriptor("openai/gpt-5.5-mini", false, true, 200_000),
                descriptor("ollama/llama3", true, false, 32_000),
            ]),
            credentialed: credentialed(&["openai"]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert_eq!(plan.model.reference(), reference("openai/gpt-5.5-mini"));
        assert!(!plan.routing.fallback_used);
        assert!(!plan.routing.override_used);
        assert_eq!(plan.routing.matched_rule, None);
        assert_eq!(plan.routing.provider, Provider::OpenAI);
        // The remaining fallback chain is everything after the chosen model.
        assert_eq!(plan.fallbacks, vec![reference("ollama/llama3")]);
    }

    #[test]
    fn should_resolve_a_matched_rule_naming_it_in_the_routing_decision() {
        let inputs = Inputs {
            input: input("refactor the auth middleware"),
            workspace: Workspace {
                referenced_paths: vec![PathBuf::from("src/auth/login.rs")],
                ..workspace()
            },
            attachments: attachments(vec![file("src/auth/login.rs", "fn login() {}", true)]),
            recalled: recalled(),
            classification: serde_json::from_value(json!({
                "default_intent": "chat",
                "rules": [{ "intent": "edit", "keywords": ["refactor"] }],
            }))
            .expect("valid classification config"),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
                "rules": [{
                    "name": "auth edits use Claude",
                    "intent": "edit",
                    "paths": ["src/auth/**"],
                    "model": "anthropic/claude-sonnet",
                }],
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                200_000,
            )]),
            credentialed: credentialed(&["anthropic"]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert_eq!(plan.classification.intent, TaskIntent::Edit);
        assert_eq!(plan.routing.intent, TaskIntent::Edit);
        assert_eq!(
            plan.routing.matched_rule.as_deref(),
            Some("auth edits use Claude")
        );
        assert_eq!(plan.model.reference(), reference("anthropic/claude-sonnet"));
    }

    #[test]
    fn should_honour_an_explicit_model_override_with_no_fallbacks() {
        let inputs = Inputs {
            input: TaskInput {
                explicit_model: Some(reference("anthropic/claude-sonnet")),
                ..input("anything")
            },
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
                "rules": [{ "name": "edits", "intent": "edit", "model": "ollama/llama3" }],
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                200_000,
            )]),
            credentialed: credentialed(&["anthropic"]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert!(plan.routing.override_used);
        assert_eq!(plan.model.reference(), reference("anthropic/claude-sonnet"));
        assert!(plan.fallbacks.is_empty());
    }

    #[test]
    fn should_walk_to_a_fallback_when_the_primary_is_unusable() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": {
                    "model": "anthropic/claude-sonnet",
                    "fallbacks": ["openai/gpt-5.5-mini", "ollama/llama3"],
                },
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![
                descriptor("anthropic/claude-sonnet", false, true, 200_000),
                descriptor("openai/gpt-5.5-mini", false, true, 200_000),
                descriptor("ollama/llama3", true, false, 32_000),
            ]),
            // Only the first fallback's provider has a credential.
            credentialed: credentialed(&["openai"]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert!(plan.routing.fallback_used);
        assert_eq!(plan.model.reference(), reference("openai/gpt-5.5-mini"));
        // The remaining chain is what comes after the chosen fallback.
        assert_eq!(plan.fallbacks, vec![reference("ollama/llama3")]);
    }

    #[test]
    fn should_force_a_local_model_when_required_context_is_restricted() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            // A required, remote-restricted file forecloses the remote primary.
            attachments: attachments(vec![file(".env", "SECRET=1", true)]),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": {
                    "model": "anthropic/claude-sonnet",
                    "fallbacks": ["ollama/llama3"],
                },
            })),
            privacy: privacy(&[".env"]),
            catalog: catalog(vec![
                descriptor("anthropic/claude-sonnet", false, true, 200_000),
                descriptor("ollama/llama3", true, false, 32_000),
            ]),
            credentialed: credentialed(&["anthropic"]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert_eq!(plan.model.reference(), reference("ollama/llama3"));
        assert!(plan.model.local);
    }

    #[test]
    fn should_force_a_local_model_when_the_caller_requires_local_only() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": {
                    "model": "anthropic/claude-sonnet",
                    "fallbacks": ["ollama/llama3"],
                },
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![
                descriptor("anthropic/claude-sonnet", false, true, 200_000),
                descriptor("ollama/llama3", true, false, 32_000),
            ]),
            credentialed: credentialed(&["anthropic"]),
            local_only: true,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert_eq!(plan.model.reference(), reference("ollama/llama3"));
    }

    #[test]
    fn should_error_when_no_route_matches_and_there_is_no_default() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "rules": [{ "name": "auth", "paths": ["src/auth/**"], "model": "ollama/llama3" }],
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![descriptor("ollama/llama3", true, false, 32_000)]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let error = Resolver::default()
            .resolve(inputs.args())
            .expect_err("no route");

        assert!(matches!(error, ResolverError::Routing(_)));
        assert_eq!(error.api_code(), ApiErrorCode::NoRoute);
    }

    #[test]
    fn should_carry_the_matched_rule_required_permissions_in_the_plan() {
        let inputs = Inputs {
            input: input("refactor the auth middleware"),
            workspace: Workspace {
                referenced_paths: vec![PathBuf::from("src/auth/login.rs")],
                ..workspace()
            },
            attachments: attachments(vec![file("src/auth/login.rs", "fn login() {}", true)]),
            recalled: recalled(),
            classification: serde_json::from_value(json!({
                "default_intent": "chat",
                "rules": [{ "intent": "edit", "keywords": ["refactor"] }],
            }))
            .expect("valid classification config"),
            routing: policy(json!({
                "default": { "model": "ollama/llama3" },
                "rules": [{
                    "name": "auth edits ask before writing",
                    "intent": "edit",
                    "paths": ["src/auth/**"],
                    "model": "ollama/llama3",
                    "required_permissions": { "permissions": { "file_write": "ask" } },
                }],
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![descriptor("ollama/llama3", true, false, 200_000)]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert_eq!(
            plan.required_permissions.mode_for("file_write"),
            Some(smista_core::policy::PermissionMode::Ask)
        );
    }

    #[test]
    fn should_leave_required_permissions_empty_for_the_default_route() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "ollama/llama3" },
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![descriptor("ollama/llama3", true, false, 32_000)]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        assert!(plan.required_permissions.permissions.is_empty());
    }

    #[test]
    fn should_map_a_forbidden_override_to_override_not_allowed() {
        let inputs = Inputs {
            input: TaskInput {
                // A remote override pinned on remote-restricted content is refused.
                explicit_model: Some(reference("anthropic/claude-sonnet")),
                ..input("hello")
            },
            workspace: workspace(),
            attachments: attachments(vec![file(".env", "SECRET=1", true)]),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "ollama/llama3" },
            })),
            privacy: privacy(&[".env"]),
            catalog: catalog(vec![descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                200_000,
            )]),
            credentialed: credentialed(&["anthropic"]),
            local_only: false,
        };

        let error = Resolver::default()
            .resolve(inputs.args())
            .expect_err("override forbidden");

        assert!(matches!(error, ResolverError::Model(_)));
        assert_eq!(error.api_code(), ApiErrorCode::OverrideNotAllowed);
    }

    #[test]
    fn should_map_an_unservable_route_to_fallback_exhausted() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
            })),
            privacy: privacy(&[]),
            // The only model needs a credential that was not supplied.
            catalog: catalog(vec![descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                200_000,
            )]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let error = Resolver::default()
            .resolve(inputs.args())
            .expect_err("no usable model");

        assert_eq!(error.api_code(), ApiErrorCode::FallbackExhausted);
    }

    #[test]
    fn should_error_when_no_model_can_serve_the_route() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            attachments: attachments(Vec::new()),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
            })),
            privacy: privacy(&[]),
            // The only model needs a credential that was not supplied.
            catalog: catalog(vec![descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                200_000,
            )]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let error = Resolver::default()
            .resolve(inputs.args())
            .expect_err("no usable model");

        assert!(matches!(error, ResolverError::Model(_)));
    }

    #[test]
    fn should_error_when_required_context_exceeds_the_model_window() {
        let inputs = Inputs {
            input: input("hello"),
            workspace: workspace(),
            // Twenty characters is five tokens of required context.
            attachments: attachments(vec![file("a.rs", "abcdefghijklmnopqrst", true)]),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "ollama/llama3" },
            })),
            privacy: privacy(&[]),
            // The five required tokens fit the window for selection, but the
            // three-token reply reserve leaves only two — too few to finalize.
            catalog: catalog(vec![descriptor_with_reply(
                "ollama/llama3",
                true,
                false,
                5,
                Some(3),
            )]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let error = Resolver::default()
            .resolve(inputs.args())
            .expect_err("context does not fit");

        assert!(matches!(error, ResolverError::Context(_)));
    }

    #[test]
    fn should_carry_all_trace_data_in_the_plan() {
        let inputs = Inputs {
            input: input("refactor"),
            workspace: workspace(),
            attachments: attachments(vec![
                file("src/main.rs", "fn main() {}", true),
                file("notes.txt", "a discardable note that will not fit", false),
            ]),
            recalled: recalled(),
            classification: serde_json::from_value(json!({
                "default_intent": "chat",
                "rules": [{ "intent": "edit", "keywords": ["refactor"] }],
            }))
            .expect("valid classification config"),
            routing: policy(json!({
                "default": { "model": "ollama/llama3" },
            })),
            privacy: privacy(&[]),
            // A tiny window (with no reply reserve) fits the three-token required
            // file but not the discardable note, so the plan carries both an
            // included and an excluded reference for the trace.
            catalog: catalog(vec![descriptor_with_reply(
                "ollama/llama3",
                true,
                false,
                3,
                Some(0),
            )]),
            credentialed: credentialed(&[]),
            local_only: false,
        };

        let plan = Resolver::default()
            .resolve(inputs.args())
            .expect("resolves");

        // Classification event data.
        assert_eq!(plan.classification.intent, TaskIntent::Edit);
        // Routing event data.
        assert_eq!(plan.routing.provider, Provider::Ollama);
        assert_eq!(plan.routing.model, "llama3");
        assert!(!plan.routing.reason.is_empty());
        // Context-selection event data: every reference names its kind and reason.
        assert!(
            plan.context
                .references
                .iter()
                .any(|reference| reference.included)
        );
        assert!(
            plan.context
                .references
                .iter()
                .any(|reference| !reference.included)
        );
        assert!(
            plan.context
                .references
                .iter()
                .all(|reference| !reference.reason.is_empty())
        );
        assert!(
            plan.context
                .included
                .iter()
                .any(|candidate| candidate.kind == CandidateKind::File)
        );
        assert!(!plan.context.outcome.included.is_empty());
    }

    #[test]
    fn should_be_deterministic_across_calls() {
        let inputs = Inputs {
            input: input("refactor the auth middleware"),
            workspace: Workspace {
                referenced_paths: vec![PathBuf::from("src/auth/login.rs")],
                ..workspace()
            },
            attachments: attachments(vec![file("src/auth/login.rs", "fn login() {}", true)]),
            recalled: recalled(),
            classification: ClassificationConfig::default(),
            routing: policy(json!({
                "default": { "model": "openai/gpt-5.5-mini", "fallbacks": ["ollama/llama3"] },
            })),
            privacy: privacy(&[]),
            catalog: catalog(vec![
                descriptor("openai/gpt-5.5-mini", false, true, 200_000),
                descriptor("ollama/llama3", true, false, 32_000),
            ]),
            credentialed: credentialed(&["openai"]),
            local_only: false,
        };

        let resolver = Resolver::default();
        let first = resolver.resolve(inputs.args()).expect("resolves");
        let second = resolver.resolve(inputs.args()).expect("resolves");

        assert_eq!(first.routing, second.routing);
        assert_eq!(first.model, second.model);
        assert_eq!(first.fallbacks, second.fallbacks);
        assert_eq!(first.context.references, second.context.references);
        assert_eq!(first.context.outcome, second.context.outcome);
    }
}
