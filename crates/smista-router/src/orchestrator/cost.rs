//! Per-turn usage pricing.
//!
//! Fills a turn's [`Usage::actual_cost`] from its token counts and the chosen
//! model's per-million-token rates. Costs are [`Decimal`], never `f64`. An
//! unpriced model (no rates) leaves the cost unset.
use rust_decimal::Decimal;
use smista_core::model::ModelDescriptor;
use smista_core::usage::Usage;

/// One million, the denominator for per-million-token rates.
const TOKENS_PER_RATE_UNIT: u64 = 1_000_000;

/// Prices `usage` against `model`, returning it with `actual_cost` filled.
///
/// When both the model's input and output rates and the turn's input and output
/// token counts are present, the cost is `input_tokens / 1_000_000 *
/// input_rate + output_tokens / 1_000_000 * output_rate`, and `currency` is set
/// to `USD`. An unpriced model, or missing token counts, leaves `usage`
/// unchanged.
pub(crate) fn priced(mut usage: Usage, model: &ModelDescriptor) -> Usage {
    let (Some(input_rate), Some(output_rate)) = (
        model.input_cost_per_million_tokens,
        model.output_cost_per_million_tokens,
    ) else {
        return usage;
    };
    let (Some(input_tokens), Some(output_tokens)) = (usage.input_tokens, usage.output_tokens)
    else {
        return usage;
    };

    let unit = Decimal::from(TOKENS_PER_RATE_UNIT);
    let cost = Decimal::from(input_tokens) / unit * input_rate
        + Decimal::from(output_tokens) / unit * output_rate;
    usage.actual_cost = Some(cost);
    usage.currency = Some("USD".to_string());
    usage
}

#[cfg(test)]
mod tests {
    use smista_core::model::{
        ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
    };

    use super::*;

    fn model_with(input: Option<Decimal>, output: Option<Decimal>) -> ModelDescriptor {
        ModelDescriptor {
            provider: Provider::Anthropic,
            model: "test".to_string(),
            display_name: None,
            local: false,
            auth: ModelAuthRequirement::default(),
            capabilities: ModelCapabilities::default(),
            max_context_tokens: 1_000,
            max_output_tokens: None,
            input_cost_per_million_tokens: input,
            output_cost_per_million_tokens: output,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    #[test]
    fn should_price_usage_from_token_counts() {
        let model = model_with(Some(Decimal::from(3)), Some(Decimal::from(15)));
        let usage = Usage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(1_000_000),
            ..Default::default()
        };
        let out = priced(usage, &model);
        assert_eq!(out.actual_cost, Some(Decimal::from(18)));
        assert_eq!(out.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn should_leave_cost_none_for_unpriced_model() {
        let model = model_with(None, None);
        let out = priced(
            Usage {
                input_tokens: Some(10),
                output_tokens: Some(10),
                ..Default::default()
            },
            &model,
        );
        assert_eq!(out.actual_cost, None);
    }
}
