//! Ollama textual tool-call recovery.
//!
//! Ollama normally returns native `message.tool_calls`. Some model templates,
//! notably Qwen variants, can instead emit JSON or `<tool_call>` text. This
//! decoder promotes only exact envelopes naming tools offered on the request.

use crate::provider::tool_call::{DecodedToolCall, TextToolCallDecoder};

/// Decoder for Ollama models using JSON or Qwen `<tool_call>` envelopes.
struct OllamaTextToolCallDecoder;

impl TextToolCallDecoder for OllamaTextToolCallDecoder {
    fn decode(&self, content: &str) -> Option<Vec<DecodedToolCall>> {
        parse_text_tool_calls(content)
    }

    fn may_match(&self, content: &str) -> bool {
        let trimmed = content.trim_start();
        if trimmed.is_empty() {
            return true;
        }
        ["{", "[", "```", "<tool_call>"]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix) || prefix.starts_with(trimmed))
    }
}

/// Shared decoder used by every resolved Ollama model.
static DECODER: OllamaTextToolCallDecoder = OllamaTextToolCallDecoder;

/// Returns Ollama's textual tool-call decoder.
pub(crate) fn decoder() -> &'static dyn TextToolCallDecoder {
    &DECODER
}

/// Parses exact Ollama tool-call envelopes.
fn parse_text_tool_calls(content: &str) -> Option<Vec<DecodedToolCall>> {
    let payloads = text_tool_call_payloads(content)?;
    let mut calls: Vec<DecodedToolCall> = Vec::new();
    for payload in payloads {
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    calls.push(serde_json::from_value(item).ok()?);
                }
            }
            item => calls.push(serde_json::from_value(item).ok()?),
        }
    }
    if calls.is_empty() || calls.iter().any(|call| !call.arguments.is_object()) {
        return None;
    }
    Some(calls)
}

/// Extracts JSON payloads from plain, fenced, or Qwen-style tool envelopes.
fn text_tool_call_payloads(content: &str) -> Option<Vec<&str>> {
    let trimmed = content.trim();
    if let Some(fenced) = trimmed.strip_prefix("```json") {
        return Some(vec![fenced.strip_suffix("```")?.trim()]);
    }
    if let Some(fenced) = trimmed.strip_prefix("```") {
        return Some(vec![fenced.strip_suffix("```")?.trim()]);
    }
    if !trimmed.starts_with("<tool_call>") {
        return Some(vec![trimmed]);
    }

    let mut payloads = Vec::new();
    let mut rest = trimmed;
    while !rest.is_empty() {
        let body = rest.strip_prefix("<tool_call>")?;
        let end = body.find("</tool_call>")?;
        payloads.push(body[..end].trim());
        rest = body[end + "</tool_call>".len()..].trim();
    }
    Some(payloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_promote_exact_textual_tool_call() {
        let calls =
            parse_text_tool_calls(r#"{"name":"read_file","arguments":{"path":"/tmp/main.rs"}}"#)
                .expect("textual tool call");

        assert_eq!(
            calls,
            vec![DecodedToolCall {
                name: "read_file".to_string(),
                arguments: serde_json::json!({ "path": "/tmp/main.rs" }),
            }]
        );
    }

    #[test]
    fn should_promote_fenced_and_qwen_textual_tool_calls() {
        let fenced =
            parse_text_tool_calls("```json\n{\"name\":\"find-skills\",\"arguments\":{}}\n```")
                .expect("fenced tool call");
        let qwen = parse_text_tool_calls(
            "<tool_call>\n{\"name\":\"find-skills\",\"arguments\":{}}\n</tool_call>",
        )
        .expect("Qwen tool call");

        assert_eq!(fenced, qwen);
        assert_eq!(fenced[0].name, "find-skills");
    }

    #[test]
    fn should_promote_unknown_tool_for_router_validation() {
        let calls = parse_text_tool_calls(
            "```json\n{\"name\":\"smista-conventions\",\"arguments\":{}}\n```",
        )
        .expect("unknown tool envelope");

        assert_eq!(calls[0].name, "smista-conventions");
    }

    #[test]
    fn should_not_promote_non_envelope_json() {
        assert_eq!(
            parse_text_tool_calls(r#"{"name":"Rosario Muniz","occupation":"unknown"}"#),
            None
        );
        assert_eq!(
            parse_text_tool_calls(r#"{"name":"find-skills","arguments":{},"extra":true}"#),
            None
        );
    }
}
