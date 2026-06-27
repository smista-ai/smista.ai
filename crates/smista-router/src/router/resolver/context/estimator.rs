//! The context stage's deterministic token estimator.
//!
//! Context selection needs a size for every candidate to rank and trim it
//! against a model's window, but no count exists upstream:
//! [`ModelDescriptor::fits_context`](smista_core::model::ModelDescriptor::fits_context)
//! consumes a count, never produces one. This module supplies that count with a
//! `chars / 4` heuristic — pure, stable and provider-agnostic. It lives in the
//! router because only the router sizes context; it is promoted to core only if
//! another crate needs it.

/// Divisor of the deterministic token estimator: roughly four characters per
/// token.
///
/// A `chars / 4` heuristic is a stable, honest approximation of a tokenizer
/// without depending on any provider's vocabulary; the router only needs a
/// consistent size to rank and trim context, not an exact count. Changing this
/// rescales every estimate, so it must stay in step with how the model windows
/// are interpreted when the candidate set is trimmed.
const CHARS_PER_TOKEN: u64 = 4;

/// Estimates the number of tokens `text` occupies, deterministically.
///
/// Counts Unicode scalar values and divides by [`CHARS_PER_TOKEN`], rounding up
/// so any non-empty text costs at least one token. Pure and stable: the same
/// input always yields the same estimate, with no tokenizer or network.
#[must_use]
pub(crate) fn estimate_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    chars.div_ceil(CHARS_PER_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_estimate_empty_text_as_zero_tokens() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn should_round_up_short_text_to_at_least_one_token() {
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn should_estimate_by_character_count_not_bytes() {
        // A multi-byte char counts as one character, not its UTF-8 byte length.
        assert_eq!(estimate_tokens("é"), 1);
        assert_eq!(estimate_tokens("🦀🦀🦀🦀"), 1);
    }

    #[test]
    fn should_be_deterministic_across_calls() {
        let text = "the quick brown fox jumps over the lazy dog";
        assert_eq!(estimate_tokens(text), estimate_tokens(text));
    }
}
