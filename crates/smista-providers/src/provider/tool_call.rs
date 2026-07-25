//! Provider-specific recovery for textual tool-call envelopes.
//!
//! Native provider tool calls remain structured throughout `rig`. This module
//! handles documented provider fallbacks where a model serializes an otherwise
//! valid tool request into assistant text.

use serde::Deserialize;
use smista_core::error::ProviderError;
use smista_core::model::Provider;
use smista_core::stream::StreamEvent;

/// A tool call decoded from provider-specific assistant text.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecodedToolCall {
    /// Name requested by the model.
    pub(crate) name: String,
    /// Arguments passed to the tool.
    pub(crate) arguments: serde_json::Value,
}

/// Recovers provider-specific textual tool envelopes.
///
/// Native structured calls never pass through this decoder. Implementations
/// exist only for providers known to emit a documented textual fallback.
pub(crate) trait TextToolCallDecoder: Sync {
    /// Decodes complete text into structured tool calls.
    fn decode(&self, content: &str) -> Option<Vec<DecodedToolCall>>;

    /// Returns whether partial streamed text can still match this format.
    fn may_match(&self, content: &str) -> bool;
}

/// Selects textual fallback decoding for providers that require it.
pub(crate) fn decoder_for_provider(
    provider: &Provider,
) -> Option<&'static dyn TextToolCallDecoder> {
    match provider {
        Provider::Ollama => Some(crate::provider::ollama::tool_call::decoder()),
        Provider::Anthropic
        | Provider::Gemini
        | Provider::OpenAI
        | Provider::OpenAICompatible(_) => None,
    }
}

/// Normalizes textual tool envelopes without delaying ordinary streamed text.
///
/// Only text that can match the selected provider format is buffered. Once the
/// prefix no longer matches, buffered text is released immediately and the
/// remaining stream passes through unchanged.
pub(crate) struct TextToolCallStream {
    /// Provider-specific fallback decoder, when the provider requires one.
    decoder: Option<&'static dyn TextToolCallDecoder>,
    /// Potential textual tool envelope; `None` means ordinary pass-through.
    candidate: Option<String>,
    /// Events that must follow buffered text or promoted calls.
    deferred: Vec<Result<StreamEvent, ProviderError>>,
    /// Whether the provider already emitted a native structured tool call.
    native_tool_call: bool,
}

impl TextToolCallStream {
    /// Creates a stream normalizer for one model request.
    pub(crate) fn new(decoder: Option<&'static dyn TextToolCallDecoder>) -> Self {
        Self {
            decoder,
            candidate: decoder.map(|_| String::new()),
            deferred: Vec::new(),
            native_tool_call: false,
        }
    }

    /// Consumes one stream item and returns zero or more normalized items.
    pub(crate) fn push(
        &mut self,
        item: Result<StreamEvent, ProviderError>,
    ) -> Vec<Result<StreamEvent, ProviderError>> {
        match item {
            Ok(StreamEvent::TextDelta { delta }) => self.push_text(delta),
            Ok(event @ StreamEvent::ToolCallStarted { .. })
            | Ok(event @ StreamEvent::ToolCallRequested { .. }) => {
                self.native_tool_call = true;
                let mut output = self.flush_candidate();
                output.push(Ok(event));
                output
            }
            Ok(StreamEvent::Usage(usage)) if self.candidate.is_some() => {
                self.deferred.push(Ok(StreamEvent::Usage(usage)));
                Vec::new()
            }
            Ok(StreamEvent::Done) => self.finish(),
            Ok(event) => vec![Ok(event)],
            Err(error) => {
                let mut output = self.flush_candidate();
                output.append(&mut self.deferred);
                output.push(Err(error));
                output
            }
        }
    }

    /// Buffers a possible envelope or releases it once it becomes ordinary text.
    fn push_text(&mut self, delta: String) -> Vec<Result<StreamEvent, ProviderError>> {
        let Some(candidate) = self.candidate.as_mut() else {
            return vec![Ok(StreamEvent::TextDelta { delta })];
        };
        candidate.push_str(&delta);
        if self
            .decoder
            .is_some_and(|decoder| decoder.may_match(candidate))
        {
            Vec::new()
        } else {
            let mut output = self.flush_candidate();
            output.append(&mut self.deferred);
            output
        }
    }

    /// Finishes the stream, promoting a valid envelope or releasing its text.
    fn finish(&mut self) -> Vec<Result<StreamEvent, ProviderError>> {
        let mut output = if self.native_tool_call {
            self.flush_candidate()
        } else if let Some(candidate) = self.candidate.take() {
            match self.decoder.and_then(|decoder| decoder.decode(&candidate)) {
                Some(calls) => {
                    tracing::debug!(
                        tool.calls = calls.len(),
                        "promoting streamed textual output to structured tool calls"
                    );
                    calls
                        .into_iter()
                        .enumerate()
                        .map(|(index, call)| {
                            Ok(StreamEvent::ToolCallRequested {
                                call_id: format!("text-tool-call-{index}"),
                                name: call.name,
                                arguments: call.arguments,
                            })
                        })
                        .collect()
                }
                None if candidate.is_empty() => Vec::new(),
                None => vec![Ok(StreamEvent::TextDelta { delta: candidate })],
            }
        } else {
            Vec::new()
        };
        output.append(&mut self.deferred);
        output.push(Ok(StreamEvent::Done));
        output
    }

    /// Releases buffered candidate text, if any, and enters pass-through mode.
    fn flush_candidate(&mut self) -> Vec<Result<StreamEvent, ProviderError>> {
        self.candidate
            .take()
            .filter(|candidate| !candidate.is_empty())
            .map(|delta| vec![Ok(StreamEvent::TextDelta { delta })])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use smista_core::usage::Usage;

    use super::*;

    #[test]
    fn should_only_enable_textual_tool_recovery_for_ollama() {
        assert!(decoder_for_provider(&Provider::Ollama).is_some());
        assert!(decoder_for_provider(&Provider::Anthropic).is_none());
        assert!(decoder_for_provider(&Provider::Gemini).is_none());
        assert!(decoder_for_provider(&Provider::OpenAI).is_none());
        assert!(decoder_for_provider(&Provider::OpenAICompatible("local".to_string())).is_none());
    }

    #[test]
    fn should_normalize_streamed_textual_tool_call_without_emitting_its_json() {
        let mut normalizer = TextToolCallStream::new(decoder_for_provider(&Provider::Ollama));
        assert!(
            normalizer
                .push(Ok(StreamEvent::TextDelta {
                    delta: "{\"name\":\"read_".to_string(),
                }))
                .is_empty()
        );
        assert!(
            normalizer
                .push(Ok(StreamEvent::TextDelta {
                    delta: "file\",\"arguments\":{\"path\":\"main.rs\"}}".to_string(),
                }))
                .is_empty()
        );
        assert!(
            normalizer
                .push(Ok(StreamEvent::Usage(Usage {
                    total_tokens: Some(12),
                    ..Default::default()
                })))
                .is_empty()
        );

        assert_eq!(
            normalizer.push(Ok(StreamEvent::Done)),
            vec![
                Ok(StreamEvent::ToolCallRequested {
                    call_id: "text-tool-call-0".to_string(),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({ "path": "main.rs" }),
                }),
                Ok(StreamEvent::Usage(Usage {
                    total_tokens: Some(12),
                    ..Default::default()
                })),
                Ok(StreamEvent::Done),
            ]
        );
    }

    #[test]
    fn should_normalize_a_fenced_unknown_tool_for_owning_layer_validation() {
        let mut normalizer = TextToolCallStream::new(decoder_for_provider(&Provider::Ollama));

        assert!(
            normalizer
                .push(Ok(StreamEvent::TextDelta {
                    delta: "\n```json\n{\"name\":\"smista-".to_string(),
                }))
                .is_empty()
        );
        assert!(
            normalizer
                .push(Ok(StreamEvent::TextDelta {
                    delta: "conventions\",\"arguments\":{}}\n```\n".to_string(),
                }))
                .is_empty()
        );

        assert_eq!(
            normalizer.push(Ok(StreamEvent::Done)),
            vec![
                Ok(StreamEvent::ToolCallRequested {
                    call_id: "text-tool-call-0".to_string(),
                    name: "smista-conventions".to_string(),
                    arguments: serde_json::json!({}),
                }),
                Ok(StreamEvent::Done),
            ]
        );
    }

    #[test]
    fn should_stream_ordinary_text_without_waiting_for_completion() {
        let mut normalizer = TextToolCallStream::new(decoder_for_provider(&Provider::Ollama));

        assert_eq!(
            normalizer.push(Ok(StreamEvent::TextDelta {
                delta: "Hello".to_string(),
            })),
            vec![Ok(StreamEvent::TextDelta {
                delta: "Hello".to_string(),
            })]
        );
        assert_eq!(
            normalizer.push(Ok(StreamEvent::TextDelta {
                delta: " world".to_string(),
            })),
            vec![Ok(StreamEvent::TextDelta {
                delta: " world".to_string(),
            })]
        );
    }
}
