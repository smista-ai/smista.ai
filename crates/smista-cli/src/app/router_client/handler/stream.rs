//! Handler for [`TurnEvent`]s streamed from the router.

use futures::StreamExt as _;
use futures::stream::BoxStream;
use smista_sdk::client::{Client as _, RouterClientError};
use smista_sdk::core::api::{
    ApprovalDecision as ApiApprovalDecision, ContentRef, ContinueRequest, EncryptedPayload,
    PendingApproval, ToolApproval, ToolRequest, TurnEvent, TurnOutcome,
};

use crate::app::router_client::approvals::ApprovalsStorage;
use crate::app::router_client::handler::continuation::UserContinuation;
use crate::app::router_client::msg::{ApprovalPrompt, AssistantTurn, ToolCallStarted};
use crate::app::router_client::state::State;
use crate::app::router_client::{
    Cmd, Msg, READ_FILE_TOOL, RouterClient, command_name, tool_changes_files,
};
use crate::tools::{ToolCall, ToolExecutor};

/// Directs the stream loop after handling one router event.
pub(in crate::app::router_client) enum StreamControl {
    /// Keep draining the current stream.
    Continue,
    /// Replace the current stream with an automatic continuation.
    Replace(BoxStream<'static, Result<TurnEvent, RouterClientError>>),
    /// Stop draining after a terminal or paused outcome.
    Stop,
}

#[cfg(test)]
impl StreamControl {
    /// Returns whether the current stream should keep draining.
    pub(in crate::app::router_client) fn is_continue(&self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Returns whether a replacement stream was opened.
    pub(in crate::app::router_client) fn is_replace(&self) -> bool {
        matches!(self, Self::Replace(_))
    }

    /// Returns whether stream draining should stop.
    pub(in crate::app::router_client) fn is_stop(&self) -> bool {
        matches!(self, Self::Stop)
    }
}

impl RouterClient {
    /// Handles the execution stream from the router.
    ///
    /// The stream can produce user-visible deltas, client-side work, or a
    /// terminal pause/completion. Text and reasoning deltas are forwarded to the
    /// UI immediately. Tool requests are either executed locally when policy
    /// allows them, or converted into an [`Msg::ApprovalPrompt`] when the user
    /// must decide first.
    ///
    /// Terminal turn outcomes update the client state, emit completion or error
    /// messages, and may open a follow-up continuation stream. Those automatic
    /// continuations cover executed tool results and router-requested E2EE
    /// decrypt/seal work, so this loop swaps to the returned stream and keeps
    /// draining until the run is done, paused, errored, or cancelled.
    pub(in crate::app::router_client) async fn handle_turn_stream(
        &mut self,
        mut stream: BoxStream<'static, Result<TurnEvent, RouterClientError>>,
    ) {
        tracing::debug!("handling execution stream");
        let mut commands_open = true;
        loop {
            tokio::select! {
                _ = self.context.exit.cancelled() => {
                    tracing::debug!("execution stream cancelled");
                    break;
                }
                maybe_cmd = self.cmd_rx.recv(), if commands_open => {
                    let Some(cmd) = maybe_cmd else {
                        tracing::debug!("router command channel closed while streaming");
                        commands_open = false;
                        continue;
                    };
                    let Cmd::Continue(continuation) = cmd else {
                        tracing::warn!(
                            command = command_name(&cmd),
                            ?self.state,
                            "received command while streaming, ignoring",
                        );
                        continue;
                    };

                    match self.open_user_continuation(continuation).await {
                        UserContinuation::Ignored => {}
                        UserContinuation::Handled if self.state == State::Idle => break,
                        UserContinuation::Handled => {}
                        UserContinuation::Replace(next_stream) => {
                            stream = next_stream;
                        }
                    }
                }
                maybe_event = stream.next() => {
                    if let Some(event) = maybe_event {
                        match self.on_exec_stream_event(event).await {
                            StreamControl::Continue => {}
                            StreamControl::Replace(next_stream) => {
                                stream = next_stream;
                            }
                            StreamControl::Stop => break,
                        }
                    } else {
                        tracing::debug!("last execution stream event received, ending stream");
                        break;
                    }
                }
            }
        }
    }

    pub(in crate::app::router_client) async fn on_exec_stream_event(
        &mut self,
        event: Result<TurnEvent, RouterClientError>,
    ) -> StreamControl {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                tracing::error!("execution stream error: {err}");
                self.send_msg(Msg::Error(format!("Execution stream error: {err}")))
                    .await;
                self.state = State::Idle;
                return StreamControl::Stop;
            }
        };

        match event {
            TurnEvent::TextDelta { delta } => {
                tracing::debug!(delta.bytes = delta.len(), "execution stream text delta");
                self.send_msg(Msg::StreamedContentChunk(delta)).await;
            }
            TurnEvent::ReasoningDelta { delta } => {
                tracing::debug!(
                    delta.bytes = delta.len(),
                    "execution stream reasoning delta"
                );
                self.send_msg(Msg::StreamedReasoningChunk(delta)).await;
            }
            TurnEvent::ToolCallStarted { call_id, name } => {
                tracing::debug!(tool.call_id = %call_id, tool.name = %name, "tool call started");
                self.send_msg(Msg::ToolCallStarted(ToolCallStarted { call_id, name }))
                    .await;
            }
            TurnEvent::ToolCallRequested {
                call_id,
                name,
                arguments,
                requires_approval,
            } => {
                self.handle_tool_call_requested(ToolRequest {
                    call_id,
                    name,
                    arguments,
                    requires_approval,
                })
                .await;
            }
            TurnEvent::Usage(usage) => {
                tracing::debug!("execution stream usage: {usage:?}");
            }
            TurnEvent::TurnEnd(turn_response) => {
                return self.handle_turn_end(*turn_response).await;
            }
        }

        StreamControl::Continue
    }

    async fn handle_awaiting_tool(&mut self, tool_requests: Vec<ToolRequest>) -> StreamControl {
        for request in &tool_requests {
            self.handle_tool_call_requested(request.clone()).await;
        }

        let has_all_results = tool_requests
            .iter()
            .all(|request| self.pending_tool_results.contains_key(&request.call_id));
        if !has_all_results {
            self.state = State::AwaitingTool;
            return StreamControl::Stop;
        }

        let results = tool_requests
            .into_iter()
            .filter_map(|request| self.pending_tool_results.remove(&request.call_id))
            .collect::<Vec<_>>();
        self.pending_tool_prompts.clear();
        self.pending_tool_requests.clear();
        self.continue_with_tool_results(results).await
    }

    async fn continue_with_tool_results(
        &mut self,
        results: Vec<smista_sdk::core::api::ToolResult>,
    ) -> StreamControl {
        let Some(session_id) = self.session_id() else {
            self.state = State::Idle;
            self.send_msg(Msg::Error(
                "Cannot submit tool results without an active session".to_owned(),
            ))
            .await;
            return StreamControl::Stop;
        };

        let key_id = self.key_id().map(str::to_owned);
        let request = match self.continue_with_api_tool_results(key_id.as_deref(), results) {
            Ok(request) => request,
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to build tool results continuation",
                );
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to build tool results continuation: {err}"
                )))
                .await;
                return StreamControl::Stop;
            }
        };

        self.stream_continue_request(session_id, request, "Failed to submit tool results")
            .await
    }

    async fn handle_tool_call_requested(&mut self, request: ToolRequest) {
        if self.pending_tool_results.contains_key(&request.call_id)
            || self.pending_tool_prompts.contains(&request.call_id)
        {
            return;
        }

        let Some(decision) = self.tool_call_decision(&request).await else {
            self.pending_tool_requests
                .insert(request.call_id.clone(), request.clone());
            self.pending_tool_prompts.insert(request.call_id.clone());
            self.state = State::AwaitingTool;
            self.send_msg(Msg::ApprovalPrompt(tool_approval_prompt(
                &self.approvals,
                &request,
            )))
            .await;
            return;
        };

        let executor = ToolExecutor::new(self.context.cwd.clone());
        let result = executor
            .execute(ToolCall {
                call_id: request.call_id.clone(),
                name: request.name,
                arguments: request.arguments,
                decision,
            })
            .await;
        self.pending_tool_results.insert(request.call_id, result);
    }

    async fn tool_call_decision(
        &self,
        request: &ToolRequest,
    ) -> Option<Option<ApiApprovalDecision>> {
        match request.requires_approval {
            ToolApproval::Allow => Some(None),
            ToolApproval::Ask if self.accept_edits && tool_changes_files(&request.name) => {
                Some(Some(ApiApprovalDecision::Approved))
            }
            ToolApproval::Ask if self.accept_reads && request.name == READ_FILE_TOOL => {
                Some(Some(ApiApprovalDecision::Approved))
            }
            ToolApproval::Ask => {
                let command = shell_command(&request.arguments)?;
                match self.approvals.approved(command) {
                    Ok(true) => Some(Some(ApiApprovalDecision::Approved)),
                    Ok(false) => None,
                    Err(err) => {
                        tracing::debug!("tool call approval lookup failed: {err}");
                        None
                    }
                }
            }
        }
    }

    async fn handle_turn_end(
        &mut self,
        turn_response: smista_sdk::core::api::TurnResponse,
    ) -> StreamControl {
        match turn_response.outcome {
            TurnOutcome::Completed(turn) => {
                tracing::debug!(trace.id = %turn.trace_id, "execution stream completed");
                let to_encrypt = turn.to_encrypt.clone();
                self.pending_tool_prompts.clear();
                self.pending_tool_requests.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                self.send_msg(Msg::AssistantTurn(AssistantTurn {
                    message: turn.message.content,
                    trace_id: Some(turn.trace_id),
                }))
                .await;
                if to_encrypt.is_empty() {
                    StreamControl::Stop
                } else {
                    self.continue_with_sealed_content(to_encrypt).await
                }
            }
            TurnOutcome::AwaitingTool {
                tool_requests,
                to_encrypt,
                ..
            } => {
                self.pending_seals = to_encrypt;
                self.handle_awaiting_tool(tool_requests).await
            }
            TurnOutcome::AwaitingApproval {
                approval,
                to_encrypt,
                ..
            } => {
                self.pending_seals = to_encrypt;
                self.state = State::AwaitingApproval;
                self.send_msg(Msg::ApprovalPrompt(approval_prompt(approval)))
                    .await;
                StreamControl::Stop
            }
            TurnOutcome::AwaitingDecrypt {
                to_decrypt,
                to_encrypt,
                trace_id,
            } => {
                tracing::debug!(trace.id = %trace_id, "execution stream awaiting decryption");
                self.continue_with_decrypted_content(to_decrypt, to_encrypt)
                    .await
            }
            TurnOutcome::AwaitingEncrypt {
                to_encrypt,
                trace_id,
            } => {
                tracing::debug!(trace.id = %trace_id, "execution stream awaiting encryption");
                self.continue_with_sealed_content(to_encrypt).await
            }
            TurnOutcome::Idle { trace_id } => {
                tracing::debug!(trace.id = %trace_id, "execution stream returned idle");
                self.pending_seals.clear();
                self.pending_tool_prompts.clear();
                self.pending_tool_requests.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                StreamControl::Stop
            }
            TurnOutcome::Error { error } => {
                tracing::error!(error.code = %error.code, "execution stream terminal error: {}", error.message);
                self.pending_seals.clear();
                self.pending_tool_prompts.clear();
                self.pending_tool_requests.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                self.send_msg(Msg::Error(error.message)).await;
                StreamControl::Stop
            }
        }
    }

    async fn continue_with_decrypted_content(
        &mut self,
        to_decrypt: std::collections::BTreeMap<ContentRef, EncryptedPayload>,
        to_encrypt: std::collections::BTreeMap<ContentRef, String>,
    ) -> StreamControl {
        let Some(session_id) = self.session_id() else {
            self.state = State::Idle;
            self.send_msg(Msg::Error(
                "Cannot submit decrypted content without an active session".to_owned(),
            ))
            .await;
            return StreamControl::Stop;
        };

        let mut plaintext = match self.decrypt_content(to_decrypt) {
            Ok(plaintext) => plaintext,
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to decrypt continuation content",
                );
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to decrypt continuation content: {err}"
                )))
                .await;
                return StreamControl::Stop;
            }
        };
        plaintext.extend(
            to_encrypt
                .iter()
                .map(|(reference, content)| (reference.clone(), content.clone())),
        );
        let key_id = self.key_id().map(str::to_owned);
        let encrypted = match self.seal_content(key_id.as_deref(), to_encrypt) {
            Ok(encrypted) => encrypted,
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to seal continuation content",
                );
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to seal continuation content: {err}"
                )))
                .await;
                return StreamControl::Stop;
            }
        };

        self.stream_continue_request(
            session_id,
            ContinueRequest::Decrypted {
                plaintext,
                encrypted,
            },
            "Failed to submit decrypted content",
        )
        .await
    }

    async fn continue_with_sealed_content(
        &mut self,
        to_encrypt: std::collections::BTreeMap<ContentRef, String>,
    ) -> StreamControl {
        let Some(session_id) = self.session_id() else {
            self.state = State::Idle;
            self.send_msg(Msg::Error(
                "Cannot submit sealed content without an active session".to_owned(),
            ))
            .await;
            return StreamControl::Stop;
        };

        let key_id = self.key_id().map(str::to_owned);
        let encrypted = match self.seal_content(key_id.as_deref(), to_encrypt) {
            Ok(encrypted) => encrypted,
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to seal continuation content",
                );
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to seal continuation content: {err}"
                )))
                .await;
                return StreamControl::Stop;
            }
        };

        self.stream_continue_request(
            session_id,
            ContinueRequest::Sealed { encrypted },
            "Failed to submit sealed content",
        )
        .await
    }

    async fn stream_continue_request(
        &mut self,
        session_id: uuid::Uuid,
        request: ContinueRequest,
        error_prefix: &str,
    ) -> StreamControl {
        match self
            .context
            .router_client
            .stream_continue(session_id, request)
            .await
        {
            Ok(stream) => {
                self.state = State::Streaming;
                StreamControl::Replace(stream)
            }
            Err(err) => {
                tracing::error!("failed to submit continuation: {err}");
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!("{error_prefix}: {err}")))
                    .await;
                StreamControl::Stop
            }
        }
    }

    fn decrypt_content(
        &self,
        to_decrypt: std::collections::BTreeMap<ContentRef, EncryptedPayload>,
    ) -> anyhow::Result<std::collections::BTreeMap<ContentRef, String>> {
        to_decrypt
            .into_iter()
            .map(|(reference, payload)| {
                self.context
                    .e2ee_keys
                    .decrypt_payload(&payload)
                    .map(|plaintext| (reference, plaintext))
            })
            .collect()
    }
}

fn shell_command(arguments: &serde_json::Value) -> Option<&str> {
    arguments.get("command").and_then(serde_json::Value::as_str)
}

fn approval_prompt(approval: PendingApproval) -> ApprovalPrompt {
    ApprovalPrompt {
        id: approval.approval_id,
        title: format!("Approve {:?}", approval.kind),
        detail: format_json_detail(&approval.detail),
        tool_name: None,
        wildcard_alias: None,
    }
}

fn tool_approval_prompt(approvals: &ApprovalsStorage, request: &ToolRequest) -> ApprovalPrompt {
    let command = shell_command(&request.arguments);
    let wildcard_alias = command.and_then(|value| match approvals.alias_for(value) {
        Ok(alias) => Some(alias),
        Err(err) => {
            tracing::debug!("tool call alias generation failed: {err}");
            None
        }
    });

    ApprovalPrompt {
        id: request.call_id.clone(),
        title: format!("Approve {}", request.name),
        detail: command
            .map(str::to_owned)
            .unwrap_or_else(|| format_tool_detail(&request.name, &request.arguments)),
        tool_name: Some(request.name.clone()),
        wildcard_alias,
    }
}

fn format_tool_detail(name: &str, arguments: &serde_json::Value) -> String {
    if name == "read_file"
        && let Some(path) = arguments.get("path").and_then(serde_json::Value::as_str)
    {
        return format!("Path: {path}");
    }
    format_json_detail(arguments)
}

fn format_json_detail(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_file_approval_detail_uses_a_readable_path() {
        let detail = format_tool_detail(
            "read_file",
            &serde_json::json!({
                "path": "crates/integration-tests/provider-integration-tests/Cargo.toml"
            }),
        );

        assert_eq!(
            detail,
            "Path: crates/integration-tests/provider-integration-tests/Cargo.toml"
        );
    }

    #[test]
    fn other_tool_approval_detail_remains_pretty_json() {
        let detail = format_tool_detail(
            "custom_tool",
            &serde_json::json!({ "first": 1, "second": true }),
        );

        assert_eq!(detail, "{\n  \"first\": 1,\n  \"second\": true\n}");
    }
}
