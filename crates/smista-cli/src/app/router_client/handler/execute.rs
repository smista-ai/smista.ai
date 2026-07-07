//! Execute command handler.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use futures::StreamExt as _;
use futures::stream::BoxStream;
use gix::bstr::ByteSlice;
use sha2::{Digest, Sha256};
use smista_sdk::client::{Client as _, RouterClientError};
use smista_sdk::core::api::{
    ApprovalDecision as ApiApprovalDecision, Attachments, ContextFile, ContextInstruction,
    ContinueRequest, ExecutePolicy, ExecuteRequest, LocalPreferences, PendingApproval, TaskInput,
    ToolApproval, ToolRequest, TurnEvent, TurnOutcome, Workspace,
};
use smista_sdk::core::intent::TaskIntent;
use smista_sdk::core::model::ModelReference;
use smista_sdk::core::skill::Skill;

use crate::app::router_client::approvals::ApprovalsStorage;
use crate::app::router_client::msg::{ApprovalPrompt, AssistantTurn, ToolCallStarted};
use crate::app::router_client::state::State;
use crate::app::router_client::{Msg, RouterClient};
use crate::skills::SkillStore;
use crate::tools::{ToolCall, ToolExecutor};

const AGENTS_MD: &str = "AGENTS.md";

impl RouterClient {
    /// Handles the execution of an `execute` command.
    ///
    /// On success, sends a [`Msg::StreamedContentChunk`] to the UI for each chunk of streamed content.
    /// On failure, sends a [`Msg::Error`] to the UI with the error message.
    pub(in crate::app::router_client) async fn execute(
        &mut self,
        prompt: String,
        files: HashSet<PathBuf>,
        plan: bool,
        explicit_model: Option<ModelReference>,
    ) {
        tracing::debug!(
            explicit_model = explicit_model.as_ref().map(ToString::to_string),
            files = ?files.iter().collect::<Vec<_>>(),
            plan,
            prompt.bytes = prompt.len(),
            "executing prompt",
        );

        let session_id = match self.session_id_or_new(&prompt).await {
            Ok(session_id) => session_id,
            Err(err) => {
                tracing::error!("failed to initialize session: {err}");
                self.send_msg(Msg::Error(format!(
                    "Failed to initialize a new session: {err}"
                )))
                .await;
                return;
            }
        };

        let execute_request = self
            .build_execute_request(prompt, files, plan, explicit_model)
            .await;

        // send Thinking state
        self.send_msg(Msg::Thinking).await;

        match self
            .context
            .router_client
            .stream_execute(session_id, execute_request)
            .await
        {
            Ok(response) => {
                tracing::debug!("execute stream accepted, starting to stream results");

                self.state = State::Streaming;
                self.handle_exec_stream(response).await;
                if self.state == State::Streaming {
                    self.state = State::Idle;
                }
            }
            Err(err) => {
                tracing::error!("failed to execute prompt: {err}");
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to execute prompt through router: {err}"
                )))
                .await;
            }
        };

        if self.state == State::Idle {
            self.send_msg(Msg::Idle).await;
        }
    }

    /// Handles the execution stream from the router.
    ///
    /// For each stream chunks:
    ///
    /// - send a [`Msg::StreamedContentChunk`] to the UI for text content
    /// - send a [`Msg::StreamedReasoningChunk`] to the UI for reasoning content
    /// - send a [`Msg::ToolRequestPrompt`] to the UI for tool call requests
    /// - send a [`Msg::ApprovalPrompt`] to the UI for approval requests
    /// - other [`TurnEvent`]s are logged for debugging purposes, but not sent to the UI.
    ///
    /// This also checks the cancellation token.
    pub(in crate::app::router_client) async fn handle_exec_stream(
        &mut self,
        mut stream: BoxStream<'static, Result<TurnEvent, RouterClientError>>,
    ) {
        tracing::debug!("handling execution stream");
        loop {
            tokio::select! {
                _ = self.context.exit.cancelled() => {
                    tracing::debug!("execution stream cancelled");
                    break;
                }
                maybe_event = stream.next() => {
                    if let Some(event) = maybe_event {
                        if let Some(next_stream) = self.on_exec_stream_event(event).await {
                            stream = next_stream;
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
    ) -> Option<BoxStream<'static, Result<TurnEvent, RouterClientError>>> {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                tracing::error!("execution stream error: {err}");
                self.send_msg(Msg::Error(format!("Execution stream error: {err}")))
                    .await;
                self.state = State::Idle;
                return None;
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

        None
    }

    async fn handle_turn_end(
        &mut self,
        turn_response: smista_sdk::core::api::TurnResponse,
    ) -> Option<BoxStream<'static, Result<TurnEvent, RouterClientError>>> {
        match turn_response.outcome {
            TurnOutcome::Completed(turn) => {
                tracing::debug!(trace.id = %turn.trace_id, "execution stream completed");
                self.pending_tool_prompts.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                self.send_msg(Msg::AssistantTurn(AssistantTurn {
                    message: turn.message.content,
                    trace_id: Some(turn.trace_id),
                }))
                .await;
                None
            }
            TurnOutcome::AwaitingTool { tool_requests, .. } => {
                self.handle_awaiting_tool(tool_requests).await
            }
            TurnOutcome::AwaitingApproval { approval, .. } => {
                self.state = State::AwaitingApproval;
                self.send_msg(Msg::ApprovalPrompt(approval_prompt(approval)))
                    .await;
                None
            }
            TurnOutcome::AwaitingDecrypt { .. } | TurnOutcome::AwaitingEncrypt { .. } => {
                self.state = State::Idle;
                self.send_msg(Msg::Error(
                    "Encrypted execution continuations are not implemented yet".to_owned(),
                ))
                .await;
                None
            }
            TurnOutcome::Idle { trace_id } => {
                tracing::debug!(trace.id = %trace_id, "execution stream returned idle");
                self.pending_tool_prompts.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                None
            }
            TurnOutcome::Error { error } => {
                tracing::error!(error.code = %error.code, "execution stream terminal error: {}", error.message);
                self.pending_tool_prompts.clear();
                self.pending_tool_results.clear();
                self.state = State::Idle;
                self.send_msg(Msg::Error(error.message)).await;
                None
            }
        }
    }

    async fn handle_awaiting_tool(
        &mut self,
        tool_requests: Vec<ToolRequest>,
    ) -> Option<BoxStream<'static, Result<TurnEvent, RouterClientError>>> {
        for request in &tool_requests {
            self.handle_tool_call_requested(request.clone()).await;
        }

        let has_all_results = tool_requests
            .iter()
            .all(|request| self.pending_tool_results.contains_key(&request.call_id));
        if !has_all_results {
            self.state = State::AwaitingTool;
            return None;
        }

        let results = tool_requests
            .into_iter()
            .filter_map(|request| self.pending_tool_results.remove(&request.call_id))
            .collect::<Vec<_>>();
        self.pending_tool_prompts.clear();
        self.continue_with_tool_results(results).await
    }

    async fn continue_with_tool_results(
        &mut self,
        results: Vec<smista_sdk::core::api::ToolResult>,
    ) -> Option<BoxStream<'static, Result<TurnEvent, RouterClientError>>> {
        let Some(session_id) = self.session_id() else {
            self.state = State::Idle;
            self.send_msg(Msg::Error(
                "Cannot submit tool results without an active session".to_owned(),
            ))
            .await;
            return None;
        };

        match self
            .context
            .router_client
            .stream_continue(
                session_id,
                ContinueRequest::ToolResults {
                    results,
                    encrypted: Default::default(),
                },
            )
            .await
        {
            Ok(stream) => {
                self.state = State::Streaming;
                Some(stream)
            }
            Err(err) => {
                tracing::error!("failed to submit tool results: {err}");
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!("Failed to submit tool results: {err}")))
                    .await;
                None
            }
        }
    }

    async fn handle_tool_call_requested(&mut self, request: ToolRequest) {
        if self.pending_tool_results.contains_key(&request.call_id)
            || self.pending_tool_prompts.contains(&request.call_id)
        {
            return;
        }

        let Some(decision) = self.tool_call_decision(&request).await else {
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

    /// Build a [`ExecuteRequest`] from the given prompt, files, and plan flag.
    pub(in crate::app::router_client) async fn build_execute_request(
        &self,
        prompt: String,
        files: HashSet<PathBuf>,
        plan: bool,
        explicit_model: Option<ModelReference>,
    ) -> ExecuteRequest {
        let command = if plan {
            tracing::debug!("planning enabled, generating plan command");
            Some(TaskIntent::Plan)
        } else {
            None
        };
        let mut referenced_paths = files.into_iter().collect::<Vec<_>>();
        referenced_paths.sort();
        let (git_branch, git_diff) = git_snapshot(&self.context.cwd);
        let attached_files = load_context_files(&self.context.cwd, &referenced_paths).await;
        let instructions = load_instructions(&self.context.cwd).await;
        let available_skills = load_available_skills(&self.context.skills_store);

        ExecuteRequest {
            input: TaskInput {
                text: prompt,
                command,
                explicit_model,
            },
            workspace: Workspace {
                root: self.context.cwd.clone(),
                git_branch,
                git_diff,
                referenced_paths,
                active_file: None,
            },
            policy: ExecutePolicy::v1(
                "merged",
                self.context.config.classification.clone(),
                self.context.config.routing.clone(),
                self.context.config.tools.clone(),
                self.context.config.privacy.clone(),
            ),
            local_preferences: LocalPreferences {
                auto_apply: self.context.config.local.auto_apply.unwrap_or_default(),
                local_only: self.context.config.local.local_only.unwrap_or_default(),
                no_network: self.context.config.local.no_network.unwrap_or_default(),
            },
            attachments: Attachments {
                files: attached_files,
                instructions,
                invoked_skills: Vec::new(),
                available_skills,
            },
        }
    }

    /// Returns the active session identifier or creates a new session for `prompt`.
    pub(in crate::app::router_client) async fn session_id_or_new(
        &mut self,
        prompt: &str,
    ) -> anyhow::Result<uuid::Uuid> {
        if let Some(session_id) = self.session_id() {
            tracing::debug!(session.id = %session_id, "reusing active session for router turn");
            Ok(session_id)
        } else {
            self.init_new_session(prompt).await
        }
    }
}

fn approval_prompt(approval: PendingApproval) -> ApprovalPrompt {
    ApprovalPrompt {
        id: approval.approval_id,
        title: format!("Approve {:?}", approval.kind),
        detail: format_json_detail(&approval.detail),
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
            .unwrap_or_else(|| format_json_detail(&request.arguments)),
        wildcard_alias,
    }
}

fn shell_command(arguments: &serde_json::Value) -> Option<&str> {
    arguments.get("command").and_then(serde_json::Value::as_str)
}

fn format_json_detail(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn git_snapshot(cwd: &Path) -> (Option<String>, Option<String>) {
    let Ok(repo) = gix::discover(cwd) else {
        return (None, None);
    };
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .and_then(|name| name.shorten().to_str().ok().map(str::to_string));
    let diff = git_diff_headers(&repo);

    (branch, diff)
}

fn git_diff_headers(repo: &gix::Repository) -> Option<String> {
    let mut paths = BTreeSet::new();
    let status = repo
        .status(gix::progress::Discard)
        .ok()?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<gix::bstr::BString>::new())
        .ok()?;

    for item in status.flatten() {
        let Ok(path) = item.location().to_str() else {
            continue;
        };
        paths.insert(path.to_string());
    }

    let mut diff = String::new();
    for path in paths {
        writeln!(&mut diff, "diff --git a/{path} b/{path}").expect("writing to String cannot fail");
    }
    (!diff.is_empty()).then_some(diff)
}

async fn load_context_files(cwd: &Path, paths: &[PathBuf]) -> Vec<ContextFile> {
    let mut files = Vec::new();
    for path in paths {
        if let Some(file) = load_context_file(cwd, path).await {
            files.push(file);
        }
    }
    files
}

async fn load_context_file(cwd: &Path, path: &Path) -> Option<ContextFile> {
    let read_path = resolve_workspace_path(cwd, path);
    let content = match tokio::fs::read_to_string(&read_path).await {
        Ok(content) => content,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error.message = %err,
                "failed to read referenced context file"
            );
            return None;
        }
    };

    Some(ContextFile {
        path: path.to_path_buf(),
        content_hash: content_hash(&content),
        content,
        required: true,
    })
}

fn resolve_workspace_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

async fn load_instructions(cwd: &Path) -> Vec<ContextInstruction> {
    let path = cwd.join(AGENTS_MD);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => vec![ContextInstruction {
            source: AGENTS_MD.to_string(),
            content,
        }],
        Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error.message = %err,
                "failed to read workspace instructions"
            );
            Vec::new()
        }
    }
}

fn load_available_skills(store: &SkillStore) -> Vec<Skill> {
    store
        .names()
        .filter_map(|name| match store.load(name) {
            Ok(skill) => Some(skill),
            Err(err) => {
                tracing::warn!(
                    skill.name = %name,
                    error.message = %err,
                    "failed to load discovered skill"
                );
                None
            }
        })
        .collect()
}

fn content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut hash = String::with_capacity("sha256:".len() + 64);
    hash.push_str("sha256:");
    for byte in digest {
        write!(&mut hash, "{byte:02x}").expect("writing to String cannot fail");
    }
    hash
}
