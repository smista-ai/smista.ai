//! The resolver: derives the deterministic routing inputs for a turn.
//!
//! Routing in smista.ai is staged. The
//! [`TaskNormalizer`](normalizer::TaskNormalizer) classifies the task intent
//! without an LLM and extracts the relevant skills and touched files into a
//! [`NormalizedTask`](normalizer::NormalizedTask). The
//! [`PolicyMatcher`](policy_matcher::PolicyMatcher) evaluates the user's routing
//! rules against that task and picks exactly one route (an override, a matched
//! rule, or the default route) as a [`RouteMatch`](policy_matcher::RouteMatch).
//! The [`ContextSelector`](context::ContextSelector) then selects the minimum
//! context the turn needs. Each stage is pure and free of side effects; the
//! execution orchestrator threads them together and turns the chosen route into
//! a usable model.

mod context;
mod model;
mod normalizer;
mod policy_matcher;
