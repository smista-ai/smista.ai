//! The resolver: derives the deterministic routing inputs for a turn.
//!
//! Routing in smista.ai is staged. The resolver's first stage is the
//! [`TaskNormalizer`](normalizer::TaskNormalizer), which classifies the task
//! intent without an LLM and extracts the relevant skills and touched files
//! into a [`NormalizedTask`](normalizer::NormalizedTask). Later stages (the
//! policy matcher, #141) consume that to select a model. The resolver is unwired
//! until the execution orchestrator (#148) drives it.

mod normalizer;
