//! Building a routing preview from a resolved turn.
//!
//! A preview answers `POST /sessions/{id}/preview`: it runs the same
//! deterministic resolve an `execute` turn would, then reports the decision
//! without ever invoking the model. [`preview_response`] maps the resolver's
//! [`ResolvedTurn`] onto the wire [`PreviewResponse`], deriving the estimated
//! cost range and the route's effective tool permissions along the way.

use rust_decimal::Decimal;
use smista_core::api::{CostRange, PreviewResponse, RequiredPermission, TaskInput};
use smista_core::policy::ToolsConfig;

use crate::router::resolver::ResolvedTurn;
use crate::router::resolver::context::estimator;

/// One million, the denominator for per-million-token rates. Matches the turn
/// loop's pricing unit so a preview and the eventual charge use the same scale.
const TOKENS_PER_RATE_UNIT: u64 = 1_000_000;

/// The reply size, in tokens, a preview assumes when the model declares no
/// output cap. The model is never called, so the upper bound of the cost range
/// needs an assumed completion length; this is a deliberately generous estimate
/// rather than a measured count.
const ASSUMED_REPLY_TOKENS: u64 = 4_096;

/// The currency every cost estimate is reported in, matching the turn loop's
/// pricing.
const CURRENCY: &str = "USD";

/// Maps a [`ResolvedTurn`] onto the wire [`PreviewResponse`].
///
/// The routing decision, classification and finalized context come straight
/// from the resolved plan; the cost range and the required permissions are
/// derived from it and the request's `base_tools` policy.
pub(super) fn preview_response(
    resolved: &ResolvedTurn,
    input: &TaskInput,
    base_tools: &ToolsConfig,
) -> PreviewResponse {
    PreviewResponse {
        task_type: resolved.routing.intent,
        classification: resolved.classification.clone(),
        provider: resolved.routing.provider.clone(),
        model: resolved.routing.model.clone(),
        matched_rule: resolved.routing.matched_rule.clone(),
        included_context: resolved.context.outcome.included.clone(),
        excluded_context: resolved.context.outcome.excluded.clone(),
        estimated_cost: estimate_cost(resolved, &input.text),
        required_permissions: required_permissions(resolved, base_tools),
    }
}

/// Estimates the cost range of serving the turn, without calling the model.
///
/// The lower bound prices only the input (the finalized context plus the
/// prompt); the upper bound adds an assumed completion. An unpriced model — a
/// local one, or any with no declared rates — reports a zero range.
fn estimate_cost(resolved: &ResolvedTurn, input_text: &str) -> CostRange {
    let input_tokens: u64 = resolved
        .context
        .included
        .iter()
        .map(|candidate| candidate.estimated_tokens)
        .sum::<u64>()
        + estimator::estimate_tokens(input_text);

    let model = &resolved.model;
    let (Some(input_rate), Some(output_rate)) = (
        model.input_cost_per_million_tokens,
        model.output_cost_per_million_tokens,
    ) else {
        return CostRange {
            min: Decimal::ZERO,
            max: Decimal::ZERO,
            currency: CURRENCY.to_string(),
        };
    };

    let unit = Decimal::from(TOKENS_PER_RATE_UNIT);
    let reply_tokens = model
        .max_output_tokens
        .map_or(ASSUMED_REPLY_TOKENS, u64::from);
    let input_cost = Decimal::from(input_tokens) / unit * input_rate;
    let max = input_cost + Decimal::from(reply_tokens) / unit * output_rate;
    CostRange {
        min: input_cost,
        max,
        currency: CURRENCY.to_string(),
    }
}

/// The effective tool permissions the route requires, for the client to show.
///
/// The project tool permissions narrowed by the route's `required_permissions`.
/// A well-formed policy only tightens, so the narrowing succeeds; a loosening
/// attempt (rejected at config validation) falls back to the project tools so a
/// preview never reports widened permissions.
fn required_permissions(
    resolved: &ResolvedTurn,
    base_tools: &ToolsConfig,
) -> Vec<RequiredPermission> {
    let effective = base_tools
        .clone()
        .narrow(&resolved.required_permissions)
        .unwrap_or_else(|_| base_tools.clone());
    effective
        .permissions
        .into_iter()
        .map(|(permission, mode)| RequiredPermission { permission, mode })
        .collect()
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use smista_core::api::TaskInput;
    use smista_core::intent::TaskIntent;
    use smista_core::model::{
        ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    };
    use smista_core::policy::{Classification, IntentSource, PermissionMode, ToolsConfig};

    use super::*;
    use crate::router::resolver::RoutingDecision;
    use crate::router::resolver::context::{
        Candidate, CandidateKind, ContextOutcome, Relevance, ResolvedContext,
    };

    fn input(text: &str) -> TaskInput {
        TaskInput {
            text: text.to_string(),
            command: None,
            explicit_model: None,
        }
    }

    /// A descriptor for the given reference with optional per-million rates.
    fn descriptor(
        reference: &str,
        input_rate: Option<Decimal>,
        output_rate: Option<Decimal>,
        max_output_tokens: Option<u32>,
    ) -> ModelDescriptor {
        let reference: ModelReference = reference.parse().expect("valid reference");
        ModelDescriptor {
            provider: reference.provider,
            model: reference.model,
            display_name: None,
            local: false,
            auth: ModelAuthRequirement::None,
            capabilities: ModelCapabilities::default(),
            max_context_tokens: 200_000,
            max_output_tokens,
            input_cost_per_million_tokens: input_rate,
            output_cost_per_million_tokens: output_rate,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    /// A resolved turn carrying the given model, required permissions and one
    /// included candidate sized at `context_tokens`.
    fn resolved(
        model: ModelDescriptor,
        required_permissions: ToolsConfig,
        context_tokens: u64,
    ) -> ResolvedTurn {
        let provider = model.provider.clone();
        let model_name = model.model.clone();
        ResolvedTurn {
            classification: Classification {
                intent: TaskIntent::Edit,
                source: IntentSource::Inferred,
                reason: "test".to_string(),
                matched_rule: Some(0),
                confidence: None,
            },
            routing: RoutingDecision {
                intent: TaskIntent::Edit,
                provider,
                model: model_name,
                matched_rule: Some("rule".to_string()),
                fallback_used: false,
                override_used: false,
                reason: "test".to_string(),
            },
            model,
            fallbacks: Vec::new(),
            context: ResolvedContext {
                included: vec![Candidate {
                    kind: CandidateKind::File,
                    path: None,
                    content: String::new(),
                    estimated_tokens: context_tokens,
                    restricted_for_remote: false,
                    required: true,
                    relevance: Relevance {
                        score: 1,
                        reason: "test".to_string(),
                    },
                }],
                outcome: ContextOutcome {
                    included: vec!["src/main.rs".to_string()],
                    excluded: vec![".env".to_string()],
                },
                references: Vec::new(),
            },
            required_permissions,
        }
    }

    #[test]
    fn should_price_the_input_and_an_assumed_reply() {
        // Two-token input text plus eight context tokens at $1/M input, and a
        // 1000-token reply cap at $2/M output.
        let model = descriptor(
            "openai/gpt-5.5-mini",
            Some(Decimal::from(1)),
            Some(Decimal::from(2)),
            Some(1_000),
        );
        let preview = preview_response(
            &resolved(model, ToolsConfig::default(), 8),
            &input("hello"),
            &ToolsConfig::default(),
        );

        // ten input tokens at 1/M = 0.000010; the reply adds 1000 * 2/M = 0.002.
        assert_eq!(
            preview.estimated_cost.min,
            "0.000010".parse::<Decimal>().unwrap()
        );
        assert_eq!(
            preview.estimated_cost.max,
            "0.002010".parse::<Decimal>().unwrap()
        );
        assert_eq!(preview.estimated_cost.currency, "USD");
    }

    #[test]
    fn should_report_a_zero_range_for_an_unpriced_model() {
        let model = descriptor("ollama/llama3", None, None, None);
        let preview = preview_response(
            &resolved(model, ToolsConfig::default(), 8),
            &input("hello"),
            &ToolsConfig::default(),
        );

        assert_eq!(preview.estimated_cost.min, Decimal::ZERO);
        assert_eq!(preview.estimated_cost.max, Decimal::ZERO);
    }

    #[test]
    fn should_narrow_the_route_permissions_over_the_base() {
        let mut base = ToolsConfig::default();
        base.set("file_read", PermissionMode::Allow);
        base.set("file_write", PermissionMode::Allow);
        let mut over = ToolsConfig::default();
        over.set("file_write", PermissionMode::Ask);

        let model = descriptor("openai/gpt-5.5-mini", None, None, None);
        let preview = preview_response(&resolved(model, over, 0), &input("hi"), &base);

        let write = preview
            .required_permissions
            .iter()
            .find(|permission| permission.permission == "file_write")
            .expect("file_write reported");
        assert_eq!(write.mode, PermissionMode::Ask);
        let read = preview
            .required_permissions
            .iter()
            .find(|permission| permission.permission == "file_read")
            .expect("file_read reported");
        assert_eq!(read.mode, PermissionMode::Allow);
    }
}
