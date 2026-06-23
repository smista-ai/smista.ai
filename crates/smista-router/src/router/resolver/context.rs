//! Context selection: the resolver's deterministic, LLM-free context stage.
//!
//! [`ContextSelector`] straddles model selection. It is **pure** and
//! **crypto-blind**: it reads nothing, calls nothing, and every content field it
//! receives is plain text. The caller recalls the history and memory and opens
//! any sealed rows *before* building the inputs here, so no crypto type ever
//! appears in this module. The behaviour is specified, user-facing, in
//! `docs/technical/context-selection.md`.
//!
//! Recall from storage and the decrypt round-trip happen in the impure loop that
//! drives the resolver; this stage only filters the plain-text material it is
//! handed.
//!
//! The stage is two methods because it straddles model selection:
//!
//! - [`ContextSelector::build_candidates`] runs *before* the model is chosen. It
//!   assembles the recalled material and the request into one
//!   [`CandidateSet`], ranks it for relevance, and marks each candidate
//!   restricted-for-remote and required-or-discardable with a size estimate.
//!   Those marks constrain model selection.
//! - [`ContextSelector::finalize`] runs *after*, trimming the set to the chosen
//!   model's window without ever dropping a required candidate.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "consumed by the execution orchestrator that drives the resolver"
    )
)]

mod estimator;

use std::path::{Path, PathBuf};

use smista_core::message::MessageRole;
use smista_core::model::ModelDescriptor;
use smista_core::policy::PrivacyPolicy;
use smista_core::skill::Skill;

use crate::router::resolver::policy_matcher::RouteMatch;
use crate::router::resolver::{Attachments, Workspace};

/// Base relevance score for an attached file.
const SCORE_FILE: u32 = 5_000;
/// Base relevance score for the workspace git diff.
const SCORE_DIFF: u32 = 4_000;
/// Base relevance score for a recalled history message.
const SCORE_MESSAGE: u32 = 3_000;
/// Base relevance score for a recalled memory fact.
const SCORE_MEMORY: u32 = 2_000;
/// Base relevance score for a skill the model may activate on its own.
const SCORE_AVAILABLE_SKILL: u32 = 1_000;
/// Bonus added when a candidate's path is in the task's focus (a referenced path
/// or a route-driving glob). Set just above [`MAX_RECENCY_BONUS`] so that
/// explicit path relevance edges out mere recency, while both stay inside the
/// candidate's kind tier.
const PATH_AFFINITY_BONUS: u32 = 500;
/// Largest recency bonus a history message can earn. Kept below
/// [`PATH_AFFINITY_BONUS`], and below the tier gap, so the freshest message
/// outranks older ones without out-weighing path affinity or crossing tiers.
const MAX_RECENCY_BONUS: u32 = 499;
/// Reply reserve assumed when a model leaves `max_output_tokens` unbounded, so
/// context never fills the whole window and starves the response.
const DEFAULT_REPLY_RESERVE_TOKENS: u64 = 256;

/// The resolver's context-selection stage: deterministic, LLM-free and pure.
///
/// A unit struct like the sibling stages ([`TaskNormalizer`] and
/// [`PolicyMatcher`]). It holds no state and owns no database: the caller
/// supplies every input, already recalled and already decrypted.
///
/// [`TaskNormalizer`]: crate::router::resolver::normalizer::TaskNormalizer
/// [`PolicyMatcher`]: crate::router::resolver::policy_matcher::PolicyMatcher
#[derive(Default, Debug)]
pub struct ContextSelector;

impl ContextSelector {
    /// Builds the relevance-ranked, privacy-marked candidate set for a turn.
    ///
    /// This is the pre-model half of context selection. It folds `recalled`
    /// (history and memory) together with the request's `attachments` and
    /// `workspace` into one [`CandidateSet`], ranks each candidate for relevance
    /// to `route`, and marks it restricted-for-remote (per `privacy`) and
    /// required-or-discardable, sizing it with [`estimator::estimate_tokens`].
    /// No model is consulted; the marks are what model selection reads to
    /// constrain its choice. All content is plain text.
    #[must_use]
    pub fn build_candidates(
        &self,
        route: &RouteMatch<'_>,
        recalled: &RecalledContext,
        attachments: &Attachments,
        workspace: &Workspace,
        privacy: &PrivacyPolicy,
    ) -> CandidateSet {
        let mut candidates = Vec::new();

        // Recalled history is supplementary (never required) and ranks by
        // recency: a newer message earns a larger bonus, so a tight budget keeps
        // the recent conversation rather than its start. A message tied to a path
        // inherits that path's remote restriction, and one about a focused path
        // is boosted.
        let message_count = recalled.messages.len();
        for (index, message) in recalled.messages.iter().enumerate() {
            let restricted = message
                .path
                .as_deref()
                .is_some_and(|path| privacy.is_restricted_for_remote(path));
            let score = SCORE_MESSAGE
                + recency_bonus(index, message_count)
                + path_affinity(route, workspace, message.path.as_deref());
            candidates.push(Candidate {
                kind: CandidateKind::Message,
                path: message.path.clone(),
                content: message.content.clone(),
                estimated_tokens: estimator::estimate_tokens(&message.content),
                restricted_for_remote: restricted,
                required: false,
                relevance: Relevance {
                    score,
                    reason: format!("{:?} message", message.role),
                },
            });
        }

        // Memory facts, user-wide and per-session alike, carry no path: they can
        // never be path-restricted and are always discardable supplementary
        // context, ranked below history.
        for fact in recalled.user_memory.iter().chain(&recalled.context_memory) {
            candidates.push(Candidate {
                kind: CandidateKind::Memory,
                path: None,
                content: fact.content.clone(),
                estimated_tokens: estimator::estimate_tokens(&fact.content),
                restricted_for_remote: false,
                required: false,
                relevance: Relevance {
                    score: SCORE_MEMORY,
                    reason: format!("memory '{}'", fact.key),
                },
            });
        }

        // Attached files are the strongest signal and rank highest. A file is
        // required, and so can never be trimmed, when the client flagged it, the
        // user referenced its path, or it matches the globs that drove the route.
        // A focused file is boosted (it is usually required anyway). Remote
        // restriction is decided purely by the path.
        for attached in &attachments.files {
            let focused = path_in_focus(route, workspace, &attached.path);
            let affinity = if focused { PATH_AFFINITY_BONUS } else { 0 };
            candidates.push(Candidate {
                kind: CandidateKind::File,
                path: Some(attached.path.clone()),
                content: attached.content.clone(),
                estimated_tokens: estimator::estimate_tokens(&attached.content),
                restricted_for_remote: privacy.is_restricted_for_remote(&attached.path),
                required: attached.required || focused,
                relevance: Relevance {
                    score: SCORE_FILE + affinity,
                    reason: format!("file '{}'", attached.path.display()),
                },
            });
        }

        // Instruction documents are authoritative: required, never trimmed. They
        // carry no path, so they are never path-restricted.
        for instruction in &attachments.instructions {
            candidates.push(Candidate {
                kind: CandidateKind::Instruction,
                path: None,
                content: instruction.content.clone(),
                estimated_tokens: estimator::estimate_tokens(&instruction.content),
                restricted_for_remote: false,
                required: true,
                relevance: Relevance {
                    score: SCORE_FILE,
                    reason: format!("instruction '{}'", instruction.source),
                },
            });
        }

        // Invoked skills are authoritative too: their instructions join the model
        // preamble, so they are required. Available skills are merely offered for
        // the model to activate on its own, so they are discardable and rank
        // lowest.
        for skill in &attachments.invoked_skills {
            candidates.push(skill_candidate(skill, true));
        }
        for skill in &attachments.available_skills {
            candidates.push(skill_candidate(skill, false));
        }

        // The workspace git diff, when present, is a single discardable candidate
        // ranked between the attachments and the recalled history.
        if let Some(diff) = &workspace.git_diff {
            candidates.push(Candidate {
                kind: CandidateKind::Diff,
                path: None,
                content: diff.clone(),
                estimated_tokens: estimator::estimate_tokens(diff),
                restricted_for_remote: false,
                required: false,
                relevance: Relevance {
                    score: SCORE_DIFF,
                    reason: "git diff".to_string(),
                },
            });
        }

        // Collapse duplicates (same path, or the same content) to one candidate,
        // keeping the required or higher-ranked copy.
        let candidates = dedupe(candidates);

        // Report the shape of the set for the trace before handing it on.
        let required = candidates.iter().filter(|c| c.required).count();
        let restricted = candidates
            .iter()
            .filter(|c| c.restricted_for_remote)
            .count();
        tracing::debug!(
            candidates = candidates.len(),
            required,
            restricted,
            "built context candidate set"
        );
        CandidateSet { candidates }
    }

    /// Trims the candidate set to fit the chosen model's context window.
    ///
    /// This is the post-model half. It keeps every required candidate plus as
    /// much relevant discardable context as fits `model`, then reports the
    /// outcome. A required candidate is never dropped to make room.
    ///
    /// # Errors
    ///
    /// Returns [`ContextError::WindowExceeded`] when the minimum required
    /// context cannot fit `model`. Model selection treats this as a
    /// fallback-eligible failure and walks the chain toward a larger window.
    pub fn finalize(
        &self,
        candidates: CandidateSet,
        model: &ModelDescriptor,
    ) -> Result<ResolvedContext, ContextError> {
        // Reserve room for the model's reply before fitting context, so the
        // response is never starved. An unbounded model gets a default reserve.
        let reply_reserve = model
            .max_output_tokens
            .map_or(DEFAULT_REPLY_RESERVE_TOKENS, u64::from);
        let budget = u64::from(model.max_context_tokens).saturating_sub(reply_reserve);

        let (required, mut discardable): (Vec<Candidate>, Vec<Candidate>) =
            candidates.candidates.into_iter().partition(|c| c.required);

        let required_tokens: u64 = required.iter().map(|c| c.estimated_tokens).sum();
        if required_tokens > budget {
            tracing::warn!(
                required_tokens,
                budget,
                "required context exceeds the effective budget; selection fails for this model"
            );
            return Err(ContextError::WindowExceeded {
                estimated: required_tokens,
                budget,
            });
        }

        // Keep the most relevant discardable context first; a stable sort keeps
        // equally relevant candidates in their recalled order.
        discardable.sort_by_key(|candidate| std::cmp::Reverse(candidate.relevance.score));

        let mut used = required_tokens;
        let mut included = required;
        let mut excluded = Vec::new();
        for candidate in discardable {
            if used + candidate.estimated_tokens <= budget {
                used += candidate.estimated_tokens;
                included.push(candidate);
            } else {
                excluded.push(candidate);
            }
        }

        let outcome = ContextOutcome {
            included: included
                .iter()
                .map(|c| c.relevance.reason.clone())
                .collect(),
            excluded: excluded
                .iter()
                .map(|c| c.relevance.reason.clone())
                .collect(),
        };
        let references = included
            .iter()
            .map(|c| reference_for(c, true))
            .chain(excluded.iter().map(|c| reference_for(c, false)))
            .collect();

        tracing::debug!(
            included = included.len(),
            excluded = excluded.len(),
            used_tokens = used,
            budget,
            "finalized context to the effective budget"
        );
        Ok(ResolvedContext {
            included,
            outcome,
            references,
        })
    }
}

/// Builds a references-only record of one context decision.
fn reference_for(candidate: &Candidate, included: bool) -> ContextReference {
    ContextReference {
        path: candidate.path.clone(),
        kind: candidate.kind,
        included,
        reason: candidate.relevance.reason.clone(),
    }
}

/// Builds a candidate for a skill: required when invoked, discardable when only
/// offered.
fn skill_candidate(skill: &Skill, invoked: bool) -> Candidate {
    let (score, role) = if invoked {
        (SCORE_FILE, "invoked")
    } else {
        (SCORE_AVAILABLE_SKILL, "available")
    };
    Candidate {
        kind: CandidateKind::Skill,
        path: None,
        content: skill.content.clone(),
        estimated_tokens: estimator::estimate_tokens(&skill.content),
        restricted_for_remote: false,
        required: invoked,
        relevance: Relevance {
            score,
            reason: format!("{role} skill '{}'", skill.name),
        },
    }
}

/// Returns whether `path` is in the task's focus: a referenced path, or a path
/// matching the globs that drove the route.
fn path_in_focus(route: &RouteMatch<'_>, workspace: &Workspace, path: &Path) -> bool {
    workspace.referenced_paths.iter().any(|p| p == path) || matches_route_paths(route, path)
}

/// The path-affinity bonus for a candidate that may carry no path.
fn path_affinity(route: &RouteMatch<'_>, workspace: &Workspace, path: Option<&Path>) -> u32 {
    match path {
        Some(path) if path_in_focus(route, workspace, path) => PATH_AFFINITY_BONUS,
        _ => 0,
    }
}

/// The recency bonus for the message at `index` of `count`, oldest first.
///
/// Scales the position into `0..=MAX_RECENCY_BONUS`, so the newest message earns
/// the most and the oldest none. A single message earns nothing.
fn recency_bonus(index: usize, count: usize) -> u32 {
    if count <= 1 {
        return 0;
    }
    let position = u64::try_from(index).unwrap_or(0);
    let span = u64::try_from(count - 1).unwrap_or(1);
    let scaled = position * u64::from(MAX_RECENCY_BONUS) / span;
    u32::try_from(scaled).unwrap_or(MAX_RECENCY_BONUS)
}

/// Collapses duplicate candidates, keeping the required or higher-ranked copy.
///
/// Two candidates collide when they share a path, or when both are pathless and
/// share their content. The kept candidate is the required one, or, when both
/// agree on required, the higher-scored one.
fn dedupe(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut deduped: Vec<Candidate> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let key = dedupe_key(&candidate);
        if let Some(existing) = deduped.iter_mut().find(|c| dedupe_key(c) == key) {
            if supersedes(&candidate, existing) {
                *existing = candidate;
            }
        } else {
            deduped.push(candidate);
        }
    }
    deduped
}

/// The identity a candidate is de-duplicated by: its path, or its content when
/// it has no path.
fn dedupe_key(candidate: &Candidate) -> String {
    match &candidate.path {
        Some(path) => format!("path:{}", path.display()),
        None => format!("content:{}", candidate.content),
    }
}

/// Returns whether `candidate` should replace `existing` when they collide.
fn supersedes(candidate: &Candidate, existing: &Candidate) -> bool {
    match (candidate.required, existing.required) {
        (true, false) => true,
        (false, true) => false,
        _ => candidate.relevance.score > existing.relevance.score,
    }
}

/// What the state machine recalled and decrypted for a turn.
///
/// Every content field is plain text: the state machine opened any sealed rows
/// before constructing this, so no crypto type appears here.
#[derive(Debug, Clone)]
pub struct RecalledContext {
    /// The session's conversation history, oldest first.
    pub messages: Vec<RecalledMessage>,
    /// User-wide memory facts.
    pub user_memory: Vec<MemoryFact>,
    /// Per-session memory facts.
    pub context_memory: Vec<MemoryFact>,
}

/// One recalled history message, with plain-text content.
///
/// History is recalled oldest first, and selection ranks messages by that order,
/// so the message carries no timestamp of its own.
#[derive(Debug, Clone)]
pub struct RecalledMessage {
    /// Who produced the message.
    pub role: MessageRole,
    /// The path the message concerns, when it has one.
    pub path: Option<PathBuf>,
    /// The plain-text content.
    pub content: String,
}

/// One recalled memory fact, with plain-text content.
#[derive(Debug, Clone)]
pub struct MemoryFact {
    /// The fact's key.
    pub key: String,
    /// The plain-text content.
    pub content: String,
}

/// The model-agnostic universe context selection filters down.
///
/// Owns its candidates: recalled rows are owned data and request-derived
/// candidates are cloned in, so the set carries no lifetime and the Resolver can
/// hold it across the model-selection call.
#[derive(Debug)]
pub struct CandidateSet {
    /// The candidates, ranked and marked.
    pub candidates: Vec<Candidate>,
}

impl CandidateSet {
    /// Whether any required candidate is restricted from remote providers.
    ///
    /// When this holds the turn must be served locally: required restricted
    /// content can never be dropped, so a remote model would inevitably receive
    /// it. Model selection reads this to foreclose remote models, exactly as
    /// `docs/technical/routing.md` specifies.
    #[must_use]
    pub fn has_restricted_required_content(&self) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.restricted_for_remote && candidate.required)
    }

    /// The token floor a model must accommodate: the sum of the required
    /// candidates' sizes.
    ///
    /// Discardable context is trimmed to fit in [`ContextSelector::finalize`], so
    /// only the required candidates set the minimum window a chosen model needs.
    #[must_use]
    pub fn required_tokens(&self) -> u64 {
        self.candidates
            .iter()
            .filter(|candidate| candidate.required)
            .map(|candidate| candidate.estimated_tokens)
            .sum()
    }
}

/// One piece of candidate context, with its relevance and privacy marks.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// What kind of context this is.
    pub kind: CandidateKind,
    /// The path it has, when any.
    pub path: Option<PathBuf>,
    /// The plain-text content. Always plain: the selector is crypto-blind.
    pub content: String,
    /// The deterministic token-size estimate.
    pub estimated_tokens: u64,
    /// Whether the content is restricted from remote providers by path.
    pub restricted_for_remote: bool,
    /// Whether the content is required (referenced or route-driving) and so
    /// cannot be dropped, as opposed to discardable supplementary context.
    pub required: bool,
    /// Why the candidate was kept, for the outcome's reason.
    pub relevance: Relevance,
}

/// The kind of a context candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    /// A history message.
    Message,
    /// An attached file.
    File,
    /// An instruction document.
    Instruction,
    /// A skill, invoked or available.
    Skill,
    /// A memory fact.
    Memory,
    /// A git diff.
    Diff,
}

/// Why a candidate was ranked in, recorded for the outcome.
#[derive(Debug, Clone)]
pub struct Relevance {
    /// The relevance score; higher ranks first.
    pub score: u32,
    /// A short, human-readable reason.
    pub reason: String,
}

/// The finalized context for a turn.
#[derive(Debug)]
pub struct ResolvedContext {
    /// The candidates that made it into the prompt.
    pub included: Vec<Candidate>,
    /// The human-readable included and excluded descriptions for the client.
    pub outcome: ContextOutcome,
    /// The references-only record of what was included or excluded.
    ///
    /// Metadata only, never the content. The state machine maps these onto
    /// `session_context_reference` rows and persists them; the selector does not
    /// know the session or user the turn belongs to.
    pub references: Vec<ContextReference>,
}

/// The human-readable summary of what context was included and excluded.
///
/// The resolver's own form, distinct from the wire
/// [`ContextOutcome`](smista_core::api::ContextOutcome): the caller maps it onto
/// the wire type for the turn response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextOutcome {
    /// Human-readable descriptions of included context.
    pub included: Vec<String>,
    /// Human-readable descriptions of excluded context.
    pub excluded: Vec<String>,
}

/// A references-only record of one context decision, for the trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextReference {
    /// The path the context had, when any.
    pub path: Option<PathBuf>,
    /// What kind of context it was.
    pub kind: CandidateKind,
    /// Whether it was included in the prompt.
    pub included: bool,
    /// Why it was included or excluded.
    pub reason: String,
}

/// An error finalizing the context for the chosen model.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// The minimum required context does not fit the model's effective budget.
    #[error("required context needs {estimated} tokens but the effective budget is {budget}")]
    WindowExceeded {
        /// The estimated token count of the required context.
        estimated: u64,
        /// The effective budget: the model's window minus the reply reserve.
        budget: u64,
    },
}

/// Returns whether `path` matches one of the route's route-driving path globs.
///
/// Only a matched rule carries paths; an override or the default route has none.
/// An invalid glob matches nothing rather than aborting, mirroring the
/// normalizer's path matching.
fn matches_route_paths(route: &RouteMatch<'_>, path: &Path) -> bool {
    let Some(rule) = route.matched_rule() else {
        return false;
    };
    if rule.paths.is_empty() {
        return false;
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in &rule.paths {
        let Ok(glob) = globset::Glob::new(pattern) else {
            return false;
        };
        builder.add(glob);
    }
    let Ok(set) = builder.build() else {
        return false;
    };
    set.is_match(path)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use smista_core::intent::TaskIntent;
    use smista_core::model::{ModelAuthRequirement, ModelCapabilities, ModelParameters, Provider};
    use smista_core::policy::{DefaultRoute, RoutingRule};

    use super::*;
    use crate::router::resolver::policy_matcher::RouteSource;
    use crate::router::resolver::{ContextFile, ContextInstruction};

    fn default_route() -> DefaultRoute {
        serde_json::from_value(json!({ "model": "ollama/llama3" })).expect("default route")
    }

    fn route_match(route: &DefaultRoute) -> RouteMatch<'_> {
        RouteMatch {
            intent: TaskIntent::Chat,
            source: RouteSource::Default { route },
            reason: "default route".to_string(),
        }
    }

    fn rule_on_paths(paths: &[&str]) -> RoutingRule {
        serde_json::from_value(json!({
            "name": "auth",
            "model": "ollama/llama3",
            "paths": paths,
        }))
        .expect("rule")
    }

    fn file(path: &str, content: &str, required: bool) -> ContextFile {
        ContextFile {
            path: PathBuf::from(path),
            content: content.to_string(),
            required,
        }
    }

    fn attachments(files: Vec<ContextFile>, instructions: Vec<ContextInstruction>) -> Attachments {
        Attachments {
            files,
            instructions,
            invoked_skills: Vec::new(),
            available_skills: Vec::new(),
        }
    }

    fn workspace(referenced: &[&str], git_diff: Option<&str>) -> Workspace {
        Workspace {
            root: PathBuf::from("/workspace"),
            git_branch: None,
            git_diff: git_diff.map(str::to_string),
            referenced_paths: referenced.iter().map(PathBuf::from).collect(),
            active_file: None,
        }
    }

    fn privacy(restricted: &[&str]) -> PrivacyPolicy {
        serde_json::from_value(json!({ "restricted_paths": restricted })).expect("privacy")
    }

    fn empty_recalled() -> RecalledContext {
        RecalledContext {
            messages: Vec::new(),
            user_memory: Vec::new(),
            context_memory: Vec::new(),
        }
    }

    fn message(content: &str) -> RecalledMessage {
        RecalledMessage {
            role: MessageRole::User,
            path: None,
            content: content.to_string(),
        }
    }

    fn message_on(content: &str, path: &str) -> RecalledMessage {
        RecalledMessage {
            path: Some(PathBuf::from(path)),
            ..message(content)
        }
    }

    fn memory(key: &str, content: &str) -> MemoryFact {
        MemoryFact {
            key: key.to_string(),
            content: content.to_string(),
        }
    }

    fn skill(name: &str, content: &str) -> Skill {
        Skill {
            name: name.to_string(),
            content: content.to_string(),
        }
    }

    fn attachments_skills(invoked: Vec<Skill>, available: Vec<Skill>) -> Attachments {
        Attachments {
            files: Vec::new(),
            instructions: Vec::new(),
            invoked_skills: invoked,
            available_skills: available,
        }
    }

    /// A model whose effective budget equals `window`, leaving no reply reserve,
    /// so trim tests can reason about exact token budgets.
    fn model(window: u32, reply: Option<u32>) -> ModelDescriptor {
        ModelDescriptor {
            provider: Provider::Ollama,
            model: "llama3".to_string(),
            display_name: None,
            local: true,
            auth: ModelAuthRequirement::None,
            capabilities: ModelCapabilities {
                streaming: true,
                tools: false,
                json_output: false,
                system_prompt: true,
                images: false,
                reasoning: false,
                memory: false,
            },
            max_context_tokens: window,
            max_output_tokens: reply,
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    fn file_candidate(set: &CandidateSet) -> &Candidate {
        set.candidates
            .iter()
            .find(|c| c.kind == CandidateKind::File)
            .expect("a file candidate")
    }

    #[test]
    fn should_mark_an_attached_required_file_as_required() {
        let route = default_route();
        let att = attachments(vec![file("src/main.rs", "fn main() {}", true)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        assert!(file_candidate(&set).required);
    }

    #[test]
    fn should_mark_a_restricted_file_by_privacy_path() {
        let route = default_route();
        let att = attachments(
            vec![
                file(".env", "SECRET=1", false),
                file("src/main.rs", "ok", false),
            ],
            Vec::new(),
        );
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[".env"]),
        );
        let env = set
            .candidates
            .iter()
            .find(|c| c.path.as_deref() == Some(Path::new(".env")))
            .expect("env candidate");
        let main = set
            .candidates
            .iter()
            .find(|c| c.path.as_deref() == Some(Path::new("src/main.rs")))
            .expect("main candidate");
        assert!(env.restricted_for_remote);
        assert!(!main.restricted_for_remote);
    }

    #[test]
    fn should_size_each_candidate_with_the_estimator() {
        let route = default_route();
        // Eight characters divided by four characters per token is two tokens.
        let att = attachments(vec![file("a.rs", "abcdefgh", false)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        assert_eq!(file_candidate(&set).estimated_tokens, 2);
    }

    #[test]
    fn should_collect_candidates_from_every_source() {
        let route = default_route();
        let recalled = RecalledContext {
            messages: vec![message("a past message")],
            user_memory: vec![memory("pref", "likes rust")],
            context_memory: Vec::new(),
        };
        let att = attachments(
            vec![file("a.rs", "code", false)],
            vec![ContextInstruction {
                source: "SMISTA.md".to_string(),
                content: "follow the rules".to_string(),
            }],
        );
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &att,
            &workspace(&[], Some("diff --git ...")),
            &privacy(&[]),
        );
        let kinds: Vec<CandidateKind> = set.candidates.iter().map(|c| c.kind).collect();
        assert!(kinds.contains(&CandidateKind::Message));
        assert!(kinds.contains(&CandidateKind::Memory));
        assert!(kinds.contains(&CandidateKind::File));
        assert!(kinds.contains(&CandidateKind::Instruction));
        assert!(kinds.contains(&CandidateKind::Diff));
    }

    #[test]
    fn should_mark_a_referenced_path_as_required() {
        let route = default_route();
        let att = attachments(vec![file("src/lib.rs", "pub fn x() {}", false)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&["src/lib.rs"], None),
            &privacy(&[]),
        );
        assert!(file_candidate(&set).required);
    }

    #[test]
    fn should_mark_a_route_driving_path_as_required() {
        let rule = rule_on_paths(&["src/auth/**"]);
        let route = RouteMatch {
            intent: TaskIntent::Edit,
            source: RouteSource::Rule {
                index: 0,
                rule: &rule,
            },
            reason: "auth rule".to_string(),
        };
        let att = attachments(vec![file("src/auth/mod.rs", "mod x;", false)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route,
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        assert!(file_candidate(&set).required);
    }

    #[test]
    fn should_keep_every_candidate_when_the_set_fits() {
        let route = default_route();
        let att = attachments(vec![file("a.rs", "tiny", false)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let resolved = ContextSelector
            .finalize(set, &model(1_000, Some(0)))
            .expect("fits");
        assert!(resolved.outcome.excluded.is_empty());
        assert_eq!(resolved.included.len(), 1);
    }

    #[test]
    fn should_drop_the_lowest_discardable_context_to_fit() {
        let route = default_route();
        // One required file (one token) plus two discardable memory facts (two
        // tokens each). A window of three fits the file and one memory fact.
        let recalled = RecalledContext {
            messages: Vec::new(),
            user_memory: vec![memory("m0", "abcdefgh"), memory("m1", "ijklmnop")],
            context_memory: Vec::new(),
        };
        let att = attachments(vec![file("a.rs", "ab", true)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let resolved = ContextSelector
            .finalize(set, &model(3, Some(0)))
            .expect("required fits");
        assert!(
            resolved
                .included
                .iter()
                .any(|c| c.kind == CandidateKind::File)
        );
        let excluded_memory = resolved
            .references
            .iter()
            .filter(|r| !r.included && r.kind == CandidateKind::Memory)
            .count();
        assert_eq!(excluded_memory, 1);
    }

    #[test]
    fn should_never_drop_a_required_candidate_to_fit() {
        let route = default_route();
        let att = attachments(
            vec![file("a.rs", "abc", true), file("b.rs", "abcdefgh", false)],
            Vec::new(),
        );
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        // A window of one token fits only the required file; the discardable one
        // is dropped, the required one is kept.
        let resolved = ContextSelector
            .finalize(set, &model(1, Some(0)))
            .expect("required fits");
        assert!(
            resolved
                .included
                .iter()
                .any(|c| c.path.as_deref() == Some(Path::new("a.rs")))
        );
        assert!(
            resolved
                .references
                .iter()
                .any(|r| !r.included && r.path.as_deref() == Some(Path::new("b.rs")))
        );
    }

    #[test]
    fn should_error_when_required_context_exceeds_the_window() {
        let route = default_route();
        // Twenty characters is five tokens; a three-token window cannot fit it.
        let att = attachments(vec![file("a.rs", "abcdefghijklmnopqrst", true)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let error = ContextSelector
            .finalize(set, &model(3, Some(0)))
            .expect_err("required exceeds window");
        assert!(matches!(
            error,
            ContextError::WindowExceeded {
                estimated: 5,
                budget: 3
            }
        ));
    }

    #[test]
    fn should_report_an_outcome_and_references_for_included_and_excluded() {
        let route = default_route();
        let recalled = RecalledContext {
            messages: Vec::new(),
            user_memory: vec![memory("m0", "abcdefgh"), memory("m1", "ijklmnop")],
            context_memory: Vec::new(),
        };
        let att = attachments(vec![file("a.rs", "ab", true)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let resolved = ContextSelector
            .finalize(set, &model(3, Some(0)))
            .expect("required fits");
        assert!(!resolved.outcome.included.is_empty());
        assert_eq!(resolved.outcome.excluded.len(), 1);
        assert!(resolved.references.iter().any(|r| r.included));
        assert!(resolved.references.iter().any(|r| !r.included));
    }

    #[test]
    fn should_keep_the_newer_message_when_only_one_fits() {
        let route = default_route();
        // Two one-token messages, oldest first; a one-token budget fits one. The
        // newer message must survive, the older one must be excluded.
        let recalled = RecalledContext {
            messages: vec![message("old"), message("new")],
            user_memory: Vec::new(),
            context_memory: Vec::new(),
        };
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &attachments(Vec::new(), Vec::new()),
            &workspace(&[], None),
            &privacy(&[]),
        );
        let resolved = ContextSelector
            .finalize(set, &model(1, Some(0)))
            .expect("one message fits");
        assert!(resolved.included.iter().any(|c| c.content == "new"));
        assert!(
            resolved
                .references
                .iter()
                .any(|r| !r.included && r.reason.contains("message"))
        );
    }

    #[test]
    fn should_require_instruction_documents() {
        let route = default_route();
        let att = attachments(
            Vec::new(),
            vec![ContextInstruction {
                source: "SMISTA.md".to_string(),
                content: "always do x".to_string(),
            }],
        );
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let instruction = set
            .candidates
            .iter()
            .find(|c| c.kind == CandidateKind::Instruction)
            .expect("an instruction candidate");
        assert!(instruction.required);
    }

    #[test]
    fn should_require_invoked_skills_but_not_available_ones() {
        let route = default_route();
        let att = attachments_skills(
            vec![skill("deploy", "how to deploy")],
            vec![skill("lint", "how to lint")],
        );
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &empty_recalled(),
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let invoked = set
            .candidates
            .iter()
            .find(|c| c.relevance.reason.contains("invoked skill"))
            .expect("invoked skill candidate");
        let available = set
            .candidates
            .iter()
            .find(|c| c.relevance.reason.contains("available skill"))
            .expect("available skill candidate");
        assert!(invoked.required);
        assert!(!available.required);
    }

    #[test]
    fn should_collapse_a_file_and_a_message_on_the_same_path() {
        let route = default_route();
        // The same path arrives twice: as an attached file and as a history
        // message. They collapse to one candidate, keeping the higher-ranked file.
        let recalled = RecalledContext {
            messages: vec![message_on("old content", "src/lib.rs")],
            user_memory: Vec::new(),
            context_memory: Vec::new(),
        };
        let att = attachments(vec![file("src/lib.rs", "pub fn x() {}", false)], Vec::new());
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &att,
            &workspace(&[], None),
            &privacy(&[]),
        );
        let on_path: Vec<&Candidate> = set
            .candidates
            .iter()
            .filter(|c| c.path.as_deref() == Some(Path::new("src/lib.rs")))
            .collect();
        assert_eq!(on_path.len(), 1);
        assert_eq!(on_path[0].kind, CandidateKind::File);
    }

    #[test]
    fn should_boost_a_message_about_a_focused_path() {
        let route = default_route();
        // The focused message is the older one, so recency favors the plain
        // message; path affinity must still rank the focused one higher.
        let recalled = RecalledContext {
            messages: vec![message_on("focused", "src/auth.rs"), message("plain")],
            user_memory: Vec::new(),
            context_memory: Vec::new(),
        };
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &attachments(Vec::new(), Vec::new()),
            &workspace(&["src/auth.rs"], None),
            &privacy(&[]),
        );
        let focused = set
            .candidates
            .iter()
            .find(|c| c.content == "focused")
            .expect("focused message");
        let plain = set
            .candidates
            .iter()
            .find(|c| c.content == "plain")
            .expect("plain message");
        assert!(focused.relevance.score > plain.relevance.score);
    }

    #[test]
    fn should_reserve_room_for_the_reply_before_fitting() {
        let route = default_route();
        // Three two-token memory facts. A ten-token window with a five-token reply
        // reserve leaves a five-token budget, which fits only two of them.
        let recalled = RecalledContext {
            messages: Vec::new(),
            user_memory: vec![
                memory("m0", "abcdefgh"),
                memory("m1", "ijklmnop"),
                memory("m2", "qrstuvwx"),
            ],
            context_memory: Vec::new(),
        };
        let set = ContextSelector.build_candidates(
            &route_match(&route),
            &recalled,
            &attachments(Vec::new(), Vec::new()),
            &workspace(&[], None),
            &privacy(&[]),
        );
        let resolved = ContextSelector
            .finalize(set, &model(10, Some(5)))
            .expect("budget holds two facts");
        assert_eq!(resolved.included.len(), 2);
        assert_eq!(resolved.outcome.excluded.len(), 1);
    }
}
