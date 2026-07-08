//! Continue command handler.

use std::collections::BTreeMap;

use anyhow::bail;
use smista_sdk::client::Client;
use smista_sdk::core::api::{
    ApprovalDecision as ApiApprovalDecision, ApprovalDecisionEntry, ContentRef, ContinueRequest,
    EncryptedPayload, ToolRequest, ToolResult, UserMessage,
};

use crate::app::router_client::state::State;
use crate::app::router_client::{Msg, RouterClient, cmd, continuation_name};
use crate::tools::{ToolCall, ToolExecutor};

impl RouterClient {
    /// Sends a continuation that answers the current router pause.
    ///
    /// Returns `true` when the command matches the current state, even if the
    /// router rejects the continuation, so the caller does not try to route the
    /// same command elsewhere.
    pub(in crate::app::router_client) async fn continue_execution(
        &mut self,
        continue_execution: cmd::ContinueExecution,
    ) -> bool {
        let Some(session) = self.session.clone() else {
            tracing::warn!(
                continuation = continuation_name(&continue_execution),
                "tried to continue execution, but no session id is present",
            );
            return false;
        };

        let session_id = session.id;
        let key_id = session.key_id.as_deref();
        tracing::debug!(
            "continuing execution with session id: {session_id}, cmd: {name}",
            name = continuation_name(&continue_execution)
        );

        let continue_request = match (&self.state, continue_execution) {
            (State::AwaitingTool, cmd::ContinueExecution::ToolResults { results }) => {
                match self.tool_results_request(key_id, results).await {
                    Ok(request) => request,
                    Err(err) => {
                        self.report_continuation_build_error(err).await;
                        return true;
                    }
                }
            }
            (State::AwaitingTool, cmd::ContinueExecution::ApprovalDecisions { decisions }) => {
                match self.tool_approval_request(key_id, decisions).await {
                    Ok(request) => request,
                    Err(err) => {
                        self.report_continuation_build_error(err).await;
                        return true;
                    }
                }
            }
            (State::AwaitingApproval, cmd::ContinueExecution::ApprovalDecisions { decisions }) => {
                match self.approval_decisions_request(key_id, decisions) {
                    Ok(request) => request,
                    Err(err) => {
                        self.report_continuation_build_error(err).await;
                        return true;
                    }
                }
            }
            (
                state @ (State::AwaitingTool | State::AwaitingApproval | State::Streaming),
                cmd::ContinueExecution::Break,
            ) => {
                tracing::debug!(?state, "break active run",);
                ContinueRequest::Break
            }
            (
                state @ (State::AwaitingTool | State::AwaitingApproval | State::Streaming),
                cmd::ContinueExecution::Inject { messages },
            ) => {
                tracing::debug!(
                    ?state,
                    message.count = messages.len(),
                    "inject user input into active run",
                );

                match self.inject_request(key_id, messages) {
                    Ok(request) => request,
                    Err(err) => {
                        self.report_continuation_build_error(err).await;
                        return true;
                    }
                }
            }
            (state, continue_execution) => {
                tracing::warn!(
                    continuation = continuation_name(&continue_execution),
                    ?state,
                    "received continuation in state, ignoring",
                );

                return false;
            }
        };

        match self
            .context
            .router_client
            .stream_continue(session_id, continue_request)
            .await
        {
            Ok(stream) => {
                self.state = State::Streaming;
                self.handle_turn_stream(stream).await;
                if self.state == State::Streaming {
                    self.state = State::Idle;
                }
            }
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to send continuation request to router",
                );
                self.send_msg(Msg::Error(format!(
                    "failed to send continuation request to router: {err}"
                )))
                .await;
            }
        }

        true
    }

    async fn tool_results_request(
        &mut self,
        key_id: Option<&str>,
        results: Vec<cmd::ToolResult>,
    ) -> anyhow::Result<ContinueRequest> {
        tracing::debug!(result.count = results.len(), "submit tool results");

        let results = results
            .into_iter()
            .map(|result| ToolResult {
                call_id: result.call_id,
                content: result.content,
                is_error: result.is_error,
                decision: None,
            })
            .collect::<Vec<_>>();
        self.continue_with_api_tool_results(key_id, results)
    }

    async fn tool_approval_request(
        &mut self,
        key_id: Option<&str>,
        decisions: Vec<cmd::ApprovalDecision>,
    ) -> anyhow::Result<ContinueRequest> {
        tracing::debug!(
            decision.count = decisions.len(),
            "submit tool approval decisions",
        );

        let mut results = Vec::with_capacity(decisions.len());
        for decision in decisions {
            let Some(request) = self.pending_tool_requests.remove(&decision.id) else {
                bail!("no pending tool request matched decision `{}`", decision.id);
            };
            self.pending_tool_prompts.remove(&decision.id);

            if decision.outcome == cmd::ApprovalOutcome::Approved
                && decision.scope == cmd::ApprovalScope::AlwaysForSession
                && let Some(command) = shell_command(&request.arguments)
            {
                self.approvals.approve(command)?;
            }

            results.push(self.run_approved_tool(request, decision).await);
        }

        self.continue_with_api_tool_results(key_id, results)
    }

    fn approval_decisions_request(
        &mut self,
        key_id: Option<&str>,
        decisions: Vec<cmd::ApprovalDecision>,
    ) -> anyhow::Result<ContinueRequest> {
        tracing::debug!(
            decision.count = decisions.len(),
            "submit approval decisions",
        );
        let decisions = decisions
            .into_iter()
            .map(|decision| ApprovalDecisionEntry {
                approval_id: decision.id,
                decision: api_approval_decision(decision.outcome),
                reason: decision.reason,
            })
            .collect();
        let encrypted = self.seal_pending_content(key_id)?;

        Ok(ContinueRequest::ApprovalDecisions {
            decisions,
            encrypted,
        })
    }

    fn inject_request(
        &self,
        key_id: Option<&str>,
        messages: Vec<cmd::UserMessage>,
    ) -> anyhow::Result<ContinueRequest> {
        let messages = messages
            .into_iter()
            .map(|message| {
                let ciphertext = key_id
                    .map(|key_id| {
                        self.context
                            .e2ee_keys
                            .encrypt_payload(key_id, &message.text)
                    })
                    .transpose()?;
                Ok(UserMessage {
                    text: message.text,
                    ciphertext,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(ContinueRequest::Inject { messages })
    }

    pub(in crate::app::router_client) fn continue_with_api_tool_results(
        &mut self,
        key_id: Option<&str>,
        results: Vec<ToolResult>,
    ) -> anyhow::Result<ContinueRequest> {
        let mut plaintext = self.pending_seals.clone();
        if key_id.is_some() {
            for result in &results {
                plaintext.insert(
                    ContentRef::ToolCall(result.call_id.clone()),
                    result.content.clone(),
                );
            }
        }
        let encrypted = self.seal_content(key_id, plaintext)?;
        self.pending_seals.clear();

        Ok(ContinueRequest::ToolResults { results, encrypted })
    }

    async fn run_approved_tool(
        &self,
        request: ToolRequest,
        decision: cmd::ApprovalDecision,
    ) -> ToolResult {
        let api_decision = api_approval_decision(decision.outcome);
        if api_decision == ApiApprovalDecision::Rejected {
            return ToolResult {
                call_id: request.call_id,
                content: decision
                    .reason
                    .unwrap_or_else(|| "Tool request rejected by user".to_owned()),
                is_error: true,
                decision: Some(api_decision),
            };
        }

        let executor = ToolExecutor::new(self.context.cwd.clone());
        executor
            .execute(ToolCall {
                call_id: request.call_id,
                name: request.name,
                arguments: request.arguments,
                decision: Some(api_decision),
            })
            .await
    }

    pub(in crate::app::router_client) fn seal_pending_content(
        &mut self,
        key_id: Option<&str>,
    ) -> anyhow::Result<BTreeMap<ContentRef, EncryptedPayload>> {
        let encrypted = self.seal_content(key_id, self.pending_seals.clone())?;
        self.pending_seals.clear();

        Ok(encrypted)
    }

    pub(in crate::app::router_client) fn seal_content(
        &self,
        key_id: Option<&str>,
        plaintext: BTreeMap<ContentRef, String>,
    ) -> anyhow::Result<BTreeMap<ContentRef, EncryptedPayload>> {
        if plaintext.is_empty() {
            return Ok(BTreeMap::new());
        }
        let Some(key_id) = key_id else {
            bail!("router asked to seal content, but the current session has no encryption key");
        };

        plaintext
            .into_iter()
            .map(|(reference, plaintext)| {
                self.context
                    .e2ee_keys
                    .encrypt_payload(key_id, &plaintext)
                    .map(|payload| (reference, payload))
            })
            .collect()
    }

    async fn report_continuation_build_error(&self, err: anyhow::Error) {
        tracing::error!(
            error = ?err,
            "failed to build continuation request",
        );
        self.send_msg(Msg::Error(format!(
            "failed to build continuation request: {err}"
        )))
        .await;
    }
}

fn api_approval_decision(outcome: cmd::ApprovalOutcome) -> ApiApprovalDecision {
    match outcome {
        cmd::ApprovalOutcome::Approved => ApiApprovalDecision::Approved,
        cmd::ApprovalOutcome::Rejected => ApiApprovalDecision::Rejected,
    }
}

fn shell_command(arguments: &serde_json::Value) -> Option<&str> {
    arguments.get("command").and_then(serde_json::Value::as_str)
}
