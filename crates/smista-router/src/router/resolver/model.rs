//! Model selection: the resolver's deterministic, LLM-free model stage.
//!
//! [`ModelSelector`] turns the [`RouteMatch`] the
//! [policy matcher](super::policy_matcher) produced into a concrete, usable
//! [`ModelDescriptor`]. It resolves the route's model against the router's known
//! model facts and the credentials supplied for the request, checks the model
//! is available and can serve the task, and — when it cannot — walks the route's
//! fallback chain in order.
//!
//! Selection is **purely deterministic and never calls an LLM**, the core
//! invariant of smista.ai. It is also **secret-blind**: it receives the set of
//! providers a credential was supplied for, never the credentials themselves,
//! which the middleware lifts from the request headers and keeps to itself.
//! Privacy is the one constraint that overrides the route: when the turn's
//! required context is restricted from remote providers, only local models are
//! eligible, and that wins over the rules and over an explicit override. The
//! behaviour is specified, user-facing, in `docs/technical/routing.md`.

use std::collections::{HashMap, HashSet};

use smista_core::model::{ModelDescriptor, ModelReference, Provider, RoutingRequirements};
use smista_core::policy::RoutingRule;

use crate::router::resolver::policy_matcher::RouteMatch;

/// An error returned when no usable model can serve a turn.
///
/// The selector never guesses a model: when the primary and every eligible
/// fallback fail, selection fails loud with one of these.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelSelectorError {
    /// The primary model and every configured fallback were unusable.
    #[error("the primary model and every fallback were exhausted")]
    FallbackExhausted,
    /// The turn's required context is restricted from remote providers, and no
    /// eligible local model could serve it.
    #[error("required context is restricted to local models, but none can serve it")]
    NoLocalModel,
    /// An explicit model override pinned a remote model on content that is
    /// restricted from remote providers.
    #[error("explicit model override `{0}` is not allowed on restricted content")]
    OverrideNotAllowed(ModelReference),
}

/// The deterministic outcome of model selection for a turn.
///
/// Carries the chosen [model](Self::model) and whether it came from the route's
/// [fallback chain](Self::fallback_used) rather than the primary model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSelection {
    /// The model the turn should be served by.
    pub model: ModelDescriptor,
    /// Whether a fallback model, rather than the route's primary, was chosen.
    pub fallback_used: bool,
}

/// Deterministic, LLM-free selector of a usable model for a matched route.
///
/// Stateless: it reads only the per-request inputs passed to
/// [`select`](Self::select).
#[derive(Debug, Default, Clone, Copy)]
pub struct ModelSelector;

impl ModelSelector {
    /// Resolves the matched route to a usable model.
    ///
    /// The candidates are the route's primary model followed by its fallback
    /// chain, in order. A candidate is chosen when its facts are known in
    /// `catalog`, it is **available** — a model that needs an API key is usable
    /// only when its provider is in `credentialed` — and it can serve
    /// `requirements` (the task's capabilities and context-window fit, unioned
    /// with the matched rule's capability gate). The first candidate that passes
    /// is returned; an unknown, uncredentialled or incapable candidate is
    /// skipped.
    ///
    /// `credentialed` is the set of providers a credential was supplied for. The
    /// selector never sees the credentials themselves: the middleware lifts them
    /// from the request headers and the router decides locally which providers
    /// they cover.
    ///
    /// When `restricted_context` is set, or the matched rule is `local_only`,
    /// only local models are eligible. A remote `input.explicit_model` override
    /// on restricted content is refused outright rather than skipped.
    ///
    /// # Errors
    ///
    /// Returns [`ModelSelectorError::OverrideNotAllowed`] for a remote override
    /// on restricted content, [`ModelSelectorError::NoLocalModel`] when local
    /// models are required but none can serve the turn, and
    /// [`ModelSelectorError::FallbackExhausted`] when the primary and every
    /// fallback are unusable.
    pub fn select(
        &self,
        route: &RouteMatch<'_>,
        requirements: &RoutingRequirements,
        catalog: &HashMap<ModelReference, ModelDescriptor>,
        credentialed: &HashSet<Provider>,
        restricted_context: bool,
    ) -> Result<ModelSelection, ModelSelectorError> {
        let local_required =
            restricted_context || route.matched_rule().is_some_and(|rule| rule.local_only);
        let requirements = effective_requirements(requirements, route.matched_rule());

        tracing::debug!(
            model = %route.model(),
            fallbacks = route.fallbacks().len(),
            override_used = route.override_used(),
            restricted_context,
            local_required,
            "selecting a model for the route"
        );

        // An explicit override bypasses the rules but never the privacy floor: a
        // remote pin on restricted content is refused outright, not skipped.
        if route.override_used()
            && restricted_context
            && catalog
                .get(route.model())
                .is_some_and(|descriptor| !descriptor.local)
        {
            return Err(ModelSelectorError::OverrideNotAllowed(
                route.model().clone(),
            ));
        }

        let candidates = std::iter::once(route.model()).chain(route.fallbacks());
        for (index, reference) in candidates.enumerate() {
            if let Some(descriptor) = usable(
                reference,
                catalog,
                credentialed,
                &requirements,
                local_required,
            ) {
                tracing::debug!(
                    model = %reference,
                    fallback_used = index > 0,
                    "selected model for the turn"
                );
                return Ok(ModelSelection {
                    model: descriptor.clone(),
                    fallback_used: index > 0,
                });
            }
        }

        if local_required {
            tracing::warn!("no eligible local model can serve the restricted turn");
            Err(ModelSelectorError::NoLocalModel)
        } else {
            tracing::warn!("the primary model and every fallback were exhausted");
            Err(ModelSelectorError::FallbackExhausted)
        }
    }
}

/// Returns the descriptor for `reference` when it can serve the turn.
///
/// A candidate is usable when its facts are known in `catalog`, any required
/// credential is present in `credentialed`, it is local when locality is
/// required, and it satisfies `requirements`.
fn usable<'a>(
    reference: &ModelReference,
    catalog: &'a HashMap<ModelReference, ModelDescriptor>,
    credentialed: &HashSet<Provider>,
    requirements: &RoutingRequirements,
    local_required: bool,
) -> Option<&'a ModelDescriptor> {
    let Some(descriptor) = catalog.get(reference) else {
        tracing::debug!(model = %reference, "skipping candidate unknown to the router");
        return None;
    };
    if descriptor.requires_api_key() && !credentialed.contains(&reference.provider) {
        tracing::debug!(
            model = %reference,
            provider = %reference.provider,
            "skipping candidate with no supplied credential"
        );
        return None;
    }
    if local_required && !descriptor.local {
        tracing::debug!(model = %reference, "skipping remote candidate on a local-only turn");
        return None;
    }
    if let Err(error) = descriptor.can_handle(requirements) {
        tracing::debug!(model = %reference, %error, "skipping candidate that cannot serve the requirements");
        return None;
    }
    Some(descriptor)
}

/// Unions the task's requirements with the matched rule's capability gate.
///
/// Every capability the rule flags is added to what the task already needs, so
/// the chosen model must satisfy both.
fn effective_requirements(
    base: &RoutingRequirements,
    rule: Option<&RoutingRule>,
) -> RoutingRequirements {
    let mut requirements = *base;
    if let Some(gate) = rule.and_then(|rule| rule.requires_capabilities) {
        requirements.streaming |= gate.streaming;
        requirements.tools |= gate.tools;
        requirements.json_output |= gate.json_output;
        requirements.system_prompt |= gate.system_prompt;
        requirements.images |= gate.images;
        requirements.reasoning |= gate.reasoning;
        requirements.memory |= gate.memory;
    }
    requirements
}

#[cfg(test)]
mod tests {
    use smista_core::intent::TaskIntent;
    use smista_core::model::{
        Capability, ModelAuthRequirement, ModelCapabilities, ModelParameters,
    };
    use smista_core::policy::DefaultRoute;

    use super::*;
    use crate::router::resolver::policy_matcher::RouteSource;

    fn reference(value: &str) -> ModelReference {
        value.parse().expect("valid model reference")
    }

    /// A descriptor for `value`, with the given locality, key requirement,
    /// capabilities and context window.
    fn descriptor(
        value: &str,
        local: bool,
        requires_key: bool,
        capabilities: ModelCapabilities,
        max_context_tokens: u32,
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
            capabilities,
            max_context_tokens,
            max_output_tokens: None,
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

    /// The set of providers a credential was supplied for, from their names.
    fn credentialed(providers: &[&str]) -> HashSet<Provider> {
        providers
            .iter()
            .map(|provider| provider.parse().expect("valid provider"))
            .collect()
    }

    fn default_route(model: &str, fallbacks: &[&str]) -> DefaultRoute {
        DefaultRoute {
            model: reference(model),
            fallbacks: fallbacks.iter().map(|value| reference(value)).collect(),
        }
    }

    fn route_match<'a>(source: RouteSource<'a>) -> RouteMatch<'a> {
        RouteMatch {
            intent: TaskIntent::Chat,
            source,
            reason: "test".to_string(),
        }
    }

    fn tools() -> ModelCapabilities {
        ModelCapabilities {
            tools: true,
            ..Default::default()
        }
    }

    #[test]
    fn should_select_primary_when_available_and_capable() {
        let route = default_route("openai/gpt-5.5-mini", &[]);
        let catalog = catalog(vec![descriptor(
            "openai/gpt-5.5-mini",
            false,
            true,
            ModelCapabilities::default(),
            200_000,
        )]);

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["openai"]),
                false,
            )
            .expect("primary is usable");

        assert_eq!(
            selection.model.reference(),
            reference("openai/gpt-5.5-mini")
        );
        assert!(!selection.fallback_used);
    }

    #[test]
    fn should_fall_back_when_primary_has_no_credential() {
        let route = default_route("anthropic/claude-sonnet", &["openai/gpt-5.5-mini"]);
        let catalog = catalog(vec![
            descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
        ]);

        // Only the fallback's provider has a credential.
        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["openai"]),
                false,
            )
            .expect("the fallback is usable");

        assert_eq!(
            selection.model.reference(),
            reference("openai/gpt-5.5-mini")
        );
        assert!(selection.fallback_used);
    }

    #[test]
    fn should_fall_back_when_primary_is_unknown_to_the_catalog() {
        let route = default_route("anthropic/claude-sonnet", &["openai/gpt-5.5-mini"]);
        // The router does not know the primary model's facts.
        let catalog = catalog(vec![descriptor(
            "openai/gpt-5.5-mini",
            false,
            true,
            ModelCapabilities::default(),
            200_000,
        )]);

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic", "openai"]),
                false,
            )
            .expect("the known fallback is usable");

        assert_eq!(
            selection.model.reference(),
            reference("openai/gpt-5.5-mini")
        );
        assert!(selection.fallback_used);
    }

    #[test]
    fn should_fall_back_when_primary_lacks_a_required_capability() {
        let route = default_route("openai/gpt-5.5-mini", &["anthropic/claude-sonnet"]);
        let catalog = catalog(vec![
            // The primary cannot call tools; the fallback can.
            descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor("anthropic/claude-sonnet", false, true, tools(), 200_000),
        ]);
        let requirements = RoutingRequirements {
            tools: true,
            ..Default::default()
        };

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &requirements,
                &catalog,
                &credentialed(&["openai", "anthropic"]),
                false,
            )
            .expect("the fallback can call tools");

        assert_eq!(
            selection.model.reference(),
            reference("anthropic/claude-sonnet")
        );
        assert!(selection.fallback_used);
    }

    #[test]
    fn should_fall_back_when_primary_context_window_is_too_small() {
        let route = default_route("openai/small", &["openai/large"]);
        let catalog = catalog(vec![
            descriptor(
                "openai/small",
                false,
                true,
                ModelCapabilities::default(),
                1_000,
            ),
            descriptor(
                "openai/large",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
        ]);
        let requirements = RoutingRequirements {
            estimated_tokens: Some(50_000),
            ..Default::default()
        };

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &requirements,
                &catalog,
                &credentialed(&["openai"]),
                false,
            )
            .expect("the larger window fits");

        assert_eq!(selection.model.reference(), reference("openai/large"));
        assert!(selection.fallback_used);
    }

    #[test]
    fn should_exhaust_when_no_candidate_is_usable() {
        let route = default_route("anthropic/claude-sonnet", &["openai/gpt-5.5-mini"]);
        let catalog = catalog(vec![
            descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
        ]);

        // No provider has a credential, so neither key-needing model is usable.
        let error = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&[]),
                false,
            )
            .expect_err("no candidate is usable");

        assert_eq!(error, ModelSelectorError::FallbackExhausted);
    }

    #[test]
    fn should_foreclose_remote_models_when_context_is_restricted() {
        let route = default_route("anthropic/claude-sonnet", &["ollama/llama3"]);
        let catalog = catalog(vec![
            descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor(
                "ollama/llama3",
                true,
                false,
                ModelCapabilities::default(),
                32_000,
            ),
        ]);

        // The remote primary is foreclosed; only the local fallback is eligible.
        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic"]),
                true,
            )
            .expect("the local fallback serves restricted content");

        assert_eq!(selection.model.reference(), reference("ollama/llama3"));
        assert!(selection.fallback_used);
    }

    #[test]
    fn should_fail_when_restricted_context_has_no_eligible_local_model() {
        let route = default_route("anthropic/claude-sonnet", &["openai/gpt-5.5-mini"]);
        let catalog = catalog(vec![
            descriptor(
                "anthropic/claude-sonnet",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
        ]);

        let error = ModelSelector
            .select(
                &route_match(RouteSource::Default { route: &route }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic", "openai"]),
                true,
            )
            .expect_err("every candidate is remote");

        assert_eq!(error, ModelSelectorError::NoLocalModel);
    }

    #[test]
    fn should_refuse_a_remote_override_on_restricted_content() {
        let pinned = reference("anthropic/claude-sonnet");
        let catalog = catalog(vec![descriptor(
            "anthropic/claude-sonnet",
            false,
            true,
            ModelCapabilities::default(),
            200_000,
        )]);

        let error = ModelSelector
            .select(
                &route_match(RouteSource::Override { model: &pinned }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic"]),
                true,
            )
            .expect_err("a remote override on restricted content is refused");

        assert_eq!(error, ModelSelectorError::OverrideNotAllowed(pinned));
    }

    #[test]
    fn should_use_an_available_override_without_fallback() {
        let pinned = reference("anthropic/claude-sonnet");
        let catalog = catalog(vec![descriptor(
            "anthropic/claude-sonnet",
            false,
            true,
            ModelCapabilities::default(),
            200_000,
        )]);

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Override { model: &pinned }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic"]),
                false,
            )
            .expect("the pinned model is usable");

        assert_eq!(selection.model.reference(), pinned);
        assert!(!selection.fallback_used);
    }

    #[test]
    fn should_keep_a_local_only_rule_fallback_chain_local() {
        // A local_only rule whose local primary is unknown to the router and
        // whose only fallback is remote: the remote fallback is foreclosed, so
        // selection fails rather than reaching the cloud.
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "secrets stay local",
            "local_only": true,
            "model": "ollama/llama3",
            "fallbacks": ["anthropic/claude-sonnet"],
        }))
        .expect("valid rule");
        // Only the remote fallback is in the catalog.
        let catalog = catalog(vec![descriptor(
            "anthropic/claude-sonnet",
            false,
            true,
            ModelCapabilities::default(),
            200_000,
        )]);

        let error = ModelSelector
            .select(
                &route_match(RouteSource::Rule {
                    index: 0,
                    rule: &rule,
                }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["anthropic"]),
                false,
            )
            .expect_err("the remote fallback is foreclosed by local_only");

        assert_eq!(error, ModelSelectorError::NoLocalModel);
    }

    #[test]
    fn should_union_the_rule_capability_gate_into_the_requirements() {
        // The rule gates on tools; the task itself needs nothing. A primary that
        // cannot call tools is rejected by the gate, falling back to one that can.
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "needs tools",
            "requires_capabilities": { "tools": true },
            "model": "openai/gpt-5.5-mini",
            "fallbacks": ["anthropic/claude-sonnet"],
        }))
        .expect("valid rule");
        let catalog = catalog(vec![
            descriptor(
                "openai/gpt-5.5-mini",
                false,
                true,
                ModelCapabilities::default(),
                200_000,
            ),
            descriptor("anthropic/claude-sonnet", false, true, tools(), 200_000),
        ]);

        let selection = ModelSelector
            .select(
                &route_match(RouteSource::Rule {
                    index: 0,
                    rule: &rule,
                }),
                &RoutingRequirements::default(),
                &catalog,
                &credentialed(&["openai", "anthropic"]),
                false,
            )
            .expect("the fallback satisfies the gate");

        assert_eq!(
            selection.model.reference(),
            reference("anthropic/claude-sonnet")
        );
        assert!(selection.fallback_used);
        // Sanity: the gate added the tools capability the task did not ask for.
        assert!(selection.model.capabilities.supports(Capability::Tools));
    }
}
