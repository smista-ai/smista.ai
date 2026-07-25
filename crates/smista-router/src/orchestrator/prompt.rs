//! Prompt assembly: resolved context + recalled history + input → messages.
//!
//! Turns the resolver's [`ResolvedContext`], the recalled history and the user
//! input into the provider-agnostic [`RequestMessage`] sequence the model is
//! invoked with. The included instruction, skill and memory candidates frame
//! the conversation in a single system message; the file and diff candidates are
//! inlined with the active user request; history and any prior-turn tool
//! exchanges are replayed in order.
use smista_core::message::MessageRole;
use smista_providers::api::RequestMessage;

use crate::router::resolver::TaskInput;
use crate::router::resolver::context::{CandidateKind, RecalledMessage, ResolvedContext};

/// Builds the provider message sequence for one turn.
///
/// Layout: one `System` message (the `preamble` followed by every included
/// instruction, skill and memory candidate), the recalled `history` as
/// `User`/`Assistant` messages, the included file and diff candidates inlined
/// with the active user `input`, then any `tool_followups` (assistant tool-call
/// and tool-result pairs accumulated earlier in this request) appended
/// verbatim.
pub(crate) fn build_messages(
    preamble: &str,
    ctx: &ResolvedContext,
    history: &[RecalledMessage],
    input: &TaskInput,
    tool_followups: &[RequestMessage],
) -> Vec<RequestMessage> {
    let mut messages = Vec::new();

    messages.push(RequestMessage::System {
        content: build_system(preamble, ctx),
    });

    messages.extend(history.iter().map(recalled_to_message));

    messages.push(RequestMessage::User {
        content: build_user_prompt(ctx, input),
    });

    messages.extend(tool_followups.iter().cloned());

    tracing::trace!(messages = messages.len(), "assembled prompt");
    messages
}

/// Builds the active prompt, keeping attached context in the same user turn.
///
/// Provider adapters distinguish the active prompt from chat history. Keeping
/// attached files here ensures providers such as Ollama receive their contents
/// as part of the request they must answer.
fn build_user_prompt(ctx: &ResolvedContext, input: &TaskInput) -> String {
    build_context_block(ctx).map_or_else(
        || input.text.clone(),
        |context| {
            format!(
                "The following context is attached and available for this request:\n\n\
                 {context}\n\nUser request:\n{}",
                input.text
            )
        },
    )
}

/// Concatenates the preamble with the included framing candidates.
fn build_system(preamble: &str, ctx: &ResolvedContext) -> String {
    let mut system = String::from(preamble);
    for candidate in &ctx.included {
        if matches!(
            candidate.kind,
            CandidateKind::Instruction | CandidateKind::Skill | CandidateKind::Memory
        ) {
            system.push_str("\n\n");
            if let Some(path) = &candidate.path {
                system.push_str(&format!("# {}\n", path.display()));
            }
            system.push_str(&candidate.content);
        }
    }
    system
}

/// Inlines the included file and diff candidates into one context block, or
/// `None` when there are none.
fn build_context_block(ctx: &ResolvedContext) -> Option<String> {
    let blocks: Vec<String> = ctx
        .included
        .iter()
        .filter(|candidate| matches!(candidate.kind, CandidateKind::File | CandidateKind::Diff))
        .map(|candidate| match &candidate.path {
            Some(path) => format!("# {}\n{}", path.display(), candidate.content),
            None => candidate.content.clone(),
        })
        .collect();
    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

/// Maps a recalled history message onto a request message by role.
fn recalled_to_message(message: &RecalledMessage) -> RequestMessage {
    match message.role {
        MessageRole::System => RequestMessage::System {
            content: message.content.clone(),
        },
        MessageRole::Assistant => RequestMessage::Assistant {
            content: message.content.clone(),
            tool_calls: Vec::new(),
        },
        MessageRole::User | MessageRole::Tool => RequestMessage::User {
            content: message.content.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::resolver::context::{Candidate, ContextOutcome, Relevance};

    fn candidate(kind: CandidateKind, content: &str) -> Candidate {
        Candidate {
            kind,
            path: None,
            content: content.to_string(),
            estimated_tokens: 0,
            restricted_for_remote: false,
            required: true,
            relevance: Relevance {
                score: 1,
                reason: "test".to_string(),
            },
        }
    }

    fn resolved_context_with(included: Vec<Candidate>) -> ResolvedContext {
        ResolvedContext {
            included,
            outcome: ContextOutcome::default(),
            references: Vec::new(),
        }
    }

    #[test]
    fn should_assemble_system_history_and_user_input() {
        let mut file = candidate(CandidateKind::File, "fn main() {}");
        file.path = Some(std::path::PathBuf::from("src/main.rs"));
        let ctx = resolved_context_with(vec![
            candidate(CandidateKind::Instruction, "AGENTS.md body"),
            file,
        ]);
        let history = vec![RecalledMessage {
            role: MessageRole::User,
            path: None,
            content: "earlier".to_string(),
        }];
        let input = TaskInput {
            text: "refactor".to_string(),
            command: None,
            explicit_model: None,
        };
        let messages = build_messages("PREAMBLE", &ctx, &history, &input, &[]);

        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages.first(),
            Some(RequestMessage::System { content })
                if content.contains("PREAMBLE") && content.contains("AGENTS.md body")
        ));
        assert!(messages.iter().any(|message| matches!(
            message,
            RequestMessage::User { content } if content.contains("earlier")
        )));
        assert!(matches!(
            messages.last(),
            Some(RequestMessage::User { content })
                if content.contains("# src/main.rs\nfn main() {}")
                    && content.ends_with("User request:\nrefactor")
        ));
    }

    #[test]
    fn should_leave_active_prompt_unchanged_without_attached_context() {
        let ctx = resolved_context_with(Vec::new());
        let input = TaskInput {
            text: "explain this".to_string(),
            command: None,
            explicit_model: None,
        };

        let messages = build_messages("PREAMBLE", &ctx, &[], &input, &[]);

        assert!(matches!(
            messages.last(),
            Some(RequestMessage::User { content }) if content == "explain this"
        ));
    }
}
