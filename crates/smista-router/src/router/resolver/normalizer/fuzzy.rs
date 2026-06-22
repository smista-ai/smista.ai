//! Typo-tolerant keyword matching.
//!
//! Keywords are matched against the prompt's *tokens* (not a raw substring scan)
//! using Optimal String Alignment (OSA) edit distance, with a static,
//! length-bucketed tolerance. Everything here is deterministic: the same inputs
//! always produce the same match, and no model is consulted. See
//! `docs/technical/task-classification.md`.

use rapidfuzz::distance::osa;

/// Largest edit distance tolerated for any keyword, whatever its length.
///
/// Caps tolerance so a long keyword can never match a wildly different token.
const MAX_FUZZY_DISTANCE: usize = 2;

/// A keyword that matched a prompt token, and how it matched.
pub(super) struct KeywordHit {
    /// The configured keyword that matched.
    pub keyword: String,
    /// `true` when the match needed a typo correction (non-zero edit distance).
    pub fuzzy: bool,
}

/// How a needle matched a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchKind {
    /// The token equals the needle.
    Exact,
    /// The token is a typo of the needle, within tolerance.
    Fuzzy,
}

/// Splits `text` into lowercase, alphanumeric tokens.
///
/// Every non-alphanumeric character is a separator, so `security-review` becomes
/// `["security", "review"]`.
pub(super) fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Finds the first keyword that matches a prompt token.
///
/// An exact match is preferred over a typo match, so an exact hit never gets its
/// confidence needlessly capped: the keywords are scanned once for an exact hit,
/// then once for a fuzzy one.
pub(super) fn match_keyword(keywords: &[String], tokens: &[String]) -> Option<KeywordHit> {
    if let Some(keyword) = keywords
        .iter()
        .find(|keyword| token_match(keyword, tokens) == Some(MatchKind::Exact))
    {
        return Some(KeywordHit {
            keyword: keyword.clone(),
            fuzzy: false,
        });
    }

    keywords
        .iter()
        .find(|keyword| token_match(keyword, tokens) == Some(MatchKind::Fuzzy))
        .map(|keyword| KeywordHit {
            keyword: keyword.clone(),
            fuzzy: true,
        })
}

/// Whether `needle` matches any of `tokens`, exactly or within typo tolerance.
pub(super) fn token_matches(needle: &str, tokens: &[String]) -> bool {
    token_match(needle, tokens).is_some()
}

/// Classifies how `needle` matches the closest of `tokens`, if at all.
fn token_match(needle: &str, tokens: &[String]) -> Option<MatchKind> {
    let needle = needle.to_lowercase();
    if tokens.contains(&needle) {
        return Some(MatchKind::Exact);
    }

    // Below the exact-only bucket there is nothing left to try.
    let cap = fuzzy_cap(needle.chars().count());
    if cap == 0 {
        return None;
    }
    tokens
        .iter()
        .any(|token| is_typo(&needle, token, cap))
        .then_some(MatchKind::Fuzzy)
}

/// Whether `token` is a typo of `needle` within `cap` edits.
///
/// A substring relationship — one string wholly contains the other — is taken to
/// be a different word, not a typo, so `preview` never matches `review`. Genuine
/// typos (substitution, transposition, internal insertion or deletion) create no
/// such relationship. OSA distance counts an adjacent transposition (`impelment`
/// vs `implement`) as a single edit.
fn is_typo(needle: &str, token: &str, cap: usize) -> bool {
    if token.contains(needle) || needle.contains(token) {
        return false;
    }
    let distance = osa::distance(needle.chars(), token.chars());
    distance != 0 && distance <= cap
}

/// The maximum tolerated edit distance for a keyword of `length` characters.
///
/// Static, non-configurable buckets following Meilisearch's stricter model:
/// keywords shorter than five characters must match exactly (so `edit` never
/// matches `audit`); longer ones tolerate one edit, and nine or more tolerate
/// two.
const fn fuzzy_cap(length: usize) -> usize {
    match length {
        0..=4 => 0,
        5..=8 => 1,
        _ => MAX_FUZZY_DISTANCE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        tokenize(text)
    }

    #[test]
    fn should_split_on_non_alphanumeric_and_lowercase() {
        assert_eq!(
            tokenize("Run a Security-Review!"),
            ["run", "a", "security", "review"]
        );
    }

    #[test]
    fn should_prefer_exact_keyword_over_fuzzy() {
        let hit = match_keyword(&["review".to_string()], &tokens("please review")).unwrap();
        assert_eq!(hit.keyword, "review");
        assert!(!hit.fuzzy);
    }

    #[test]
    fn should_match_keyword_through_a_transposition() {
        let hit = match_keyword(&["implement".to_string()], &tokens("impelment it")).unwrap();
        assert!(hit.fuzzy);
    }

    #[test]
    fn should_not_match_when_a_word_merely_contains_the_keyword() {
        assert!(match_keyword(&["review".to_string()], &tokens("preview the page")).is_none());
    }

    #[test]
    fn should_keep_short_keywords_exact_only() {
        assert!(match_keyword(&["edit".to_string()], &tokens("audit it")).is_none());
        assert!(match_keyword(&["edit".to_string()], &tokens("edit it")).is_some());
    }

    #[test]
    fn should_respect_the_length_buckets() {
        assert_eq!(fuzzy_cap(4), 0);
        assert_eq!(fuzzy_cap(5), 1);
        assert_eq!(fuzzy_cap(8), 1);
        assert_eq!(fuzzy_cap(9), 2);
    }
}
