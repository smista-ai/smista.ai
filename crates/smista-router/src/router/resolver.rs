//! The resolver: derives the deterministic routing inputs for a turn.
//!
//! Routing in smista.ai is staged. The resolver's first stage is the
//! [`TaskNormalizer`](normalizer::TaskNormalizer), which classifies the task
//! intent without an LLM and extracts the relevant skills and touched files
//! into a [`NormalizedTask`](normalizer::NormalizedTask). The second stage is
//! the [`PolicyMatcher`](policy_matcher::PolicyMatcher), which evaluates the
//! user's routing rules against that task and picks exactly one route — an
//! override, a matched rule, or the default route — as a
//! [`RouteMatch`](policy_matcher::RouteMatch). Model selection (#142) then turns
//! that route into a usable model. The resolver is unwired until the execution
//! orchestrator (#148) drives it.

mod normalizer;
mod policy_matcher;
