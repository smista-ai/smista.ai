//! TUI State

#![expect(
    dead_code,
    reason = "TUI state is implemented before the renderer consumes every state branch."
)]

mod active_component;
mod console;
mod history;
mod list;
mod prompt;
mod router;
mod turn;

use std::collections::VecDeque;

use smista_sdk::core::model::ModelReference;

pub use self::active_component::{ActiveComponentKind, ActiveComponentState, UsageState};
pub use self::console::ConsoleState;
pub use self::history::HistoryEntry;
pub use self::list::ListState;
pub use self::prompt::{Command, PromptState};
pub use self::router::RouterState;
pub use self::turn::ExecutionTurn;
use crate::app::router_client::Msg;
use crate::app::router_client::msg::{Model, Provider, SessionListItem, TraceEvent};
use crate::skills::SkillEntry;

const COMPONENT_MODELS_LIST: &str = "models_list";
const COMPONENT_CONSOLE: &str = "console";
const COMPONENT_PROVIDERS_LIST: &str = "providers_list";
const COMPONENT_SESSIONS_LIST: &str = "sessions_list";
const COMPONENT_SKILL_LIST: &str = "skill_list";
const COMPONENT_TRACING_LIST: &str = "tracing_list";
const COMPONENT_USAGE: &str = "usage";
const MESSAGE_APPROVAL_PROMPT: &str = "approval_prompt";
const MESSAGE_ASSISTANT_TURN: &str = "assistant_turn";
const MESSAGE_ERROR: &str = "error";
const MESSAGE_IDLE: &str = "idle";
const MESSAGE_INTERRUPTED: &str = "interrupted";
const MESSAGE_MODELS_LIST: &str = "models_list";
const MESSAGE_PREVIEW: &str = "preview";
const MESSAGE_PROVIDERS_LIST: &str = "providers_list";
const MESSAGE_RESUMED_SESSION: &str = "resumed_session";
const MESSAGE_ROUTER_STATUS: &str = "router_status";
const MESSAGE_SESSIONS_LIST: &str = "sessions_list";
const MESSAGE_STREAMED_CONTENT_CHUNK: &str = "streamed_content_chunk";
const MESSAGE_STREAMED_REASONING_CHUNK: &str = "streamed_reasoning_chunk";
const MESSAGE_THINKING: &str = "thinking";
const MESSAGE_TOOL_CALL_STARTED: &str = "tool_call_started";
const MESSAGE_TRACE: &str = "trace";
const MESSAGE_USAGE: &str = "usage";
const PROMPT_HISTORY_LIMIT: usize = 100;
const ROUTER_NOTICE_PREFIX: &str = "router";

/// Selectable entry in the model picker.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelListEntry {
    /// Automatic model routing.
    Auto,
    /// Explicit model choice.
    Model(Model),
}

/// Result of moving through prompt input history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptHistoryNavigation {
    /// The prompt should be replaced with this history entry.
    Entry(String),
    /// Navigation moved past the newest entry, so the prompt should clear.
    Clear,
    /// There was no active history navigation to update.
    Unchanged,
}

/// Terminal UI state for the smista-cli user interface.
#[derive(Debug, Default)]
pub struct State {
    /// State of the active component in the TUI.
    ///
    /// On active component change, the previous state is lost.
    pub active_component: ActiveComponentState,
    /// Set awaited model. This means the user has selected a model, but we need to await list of models to validate it.
    pub awaited_model: Option<String>,
    /// The active execution turn, if any
    pub execution_turn: Option<ExecutionTurn>,
    /// Conversation history
    pub history: Vec<HistoryEntry>,
    /// Preferred model selected by the user.
    preferred_model: Option<ModelReference>,
    /// Previously submitted non-empty prompt inputs.
    prompt_history: VecDeque<String>,
    /// Currently recalled prompt history index.
    prompt_history_index: Option<usize>,
    /// The state of the smista-router execution ()
    pub router: RouterState,
}

impl State {
    /// Appends a renderable history entry.
    pub fn push_history(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        tracing::trace!(
            history.entries = self.history.len(),
            "appended tui history entry"
        );
    }

    /// Appends a non-empty prompt input to recall history.
    pub fn push_prompt_history(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        if self.prompt_history.len() == PROMPT_HISTORY_LIMIT {
            self.prompt_history.pop_front();
        }

        self.prompt_history.push_back(input);
        self.reset_prompt_history_navigation();
        tracing::trace!(
            prompt_history.entries = self.prompt_history.len(),
            "appended prompt history entry"
        );
    }

    /// Moves to the previous prompt history entry.
    #[must_use]
    pub fn previous_prompt_history_entry(&mut self) -> Option<String> {
        if self.prompt_history.is_empty() {
            return None;
        }

        let index = self.prompt_history_index.map_or_else(
            || self.prompt_history.len().saturating_sub(1),
            |index| index.saturating_sub(1),
        );
        self.prompt_history_index = Some(index);

        self.prompt_history.get(index).cloned()
    }

    /// Moves to the next prompt history entry.
    #[must_use]
    pub fn next_prompt_history_entry(&mut self) -> PromptHistoryNavigation {
        let Some(index) = self.prompt_history_index else {
            return PromptHistoryNavigation::Unchanged;
        };

        let next_index = index.saturating_add(1);
        if next_index >= self.prompt_history.len() {
            self.reset_prompt_history_navigation();
            return PromptHistoryNavigation::Clear;
        }

        self.prompt_history_index = Some(next_index);
        self.prompt_history.get(next_index).cloned().map_or(
            PromptHistoryNavigation::Clear,
            PromptHistoryNavigation::Entry,
        )
    }

    /// Returns `true` when a prompt history entry is currently recalled.
    #[must_use]
    pub fn is_prompt_history_navigation_active(&self) -> bool {
        self.prompt_history_index.is_some()
    }

    /// Stops navigating prompt history.
    pub fn reset_prompt_history_navigation(&mut self) {
        self.prompt_history_index = None;
    }

    /// Clears history for a session change.
    pub fn clear_history(&mut self) {
        let previous_history_entries = self.history.len();
        self.history.clear();
        self.execution_turn = None;
        tracing::debug!(
            history.previous_entries = previous_history_entries,
            "cleared tui history"
        );
    }

    /// Returns the preferred model selected by the user.
    #[must_use]
    pub fn preferred_model(&self) -> Option<&ModelReference> {
        self.preferred_model.as_ref()
    }

    /// Sets the preferred model selected by the user.
    pub fn set_preferred_model(&mut self, model: ModelReference) {
        self.preferred_model = Some(model);
        tracing::debug!(preferred_model.present = true, "set tui preferred model");
    }

    /// Returns the awaited model selected by the user.
    ///
    /// Replaces the awaited model with None, so it can only be called once per awaited model.
    #[must_use]
    pub fn take_awaited_model(&mut self) -> Option<String> {
        self.awaited_model.take()
    }

    /// Sets the awaited model selected by the user.
    pub fn set_awaited_model(&mut self, model: String) {
        self.awaited_model = Some(model);
    }

    /// Clears the preferred model selected by the user.
    pub fn clear_preferred_model(&mut self) {
        self.preferred_model = None;
        tracing::debug!(
            preferred_model.present = false,
            "cleared tui preferred model"
        );
    }

    /// Restores the main console view without changing history.
    pub fn show_console(&mut self) {
        self.active_component = ActiveComponentState::Console(ConsoleState::default());
        self.trace_active_component(COMPONENT_CONSOLE, None);
    }

    /// Shows the skill list view without changing history.
    pub fn show_skill_list(&mut self, skills: Vec<(String, SkillEntry)>) {
        let entry_count = skills.len();
        self.active_component = ActiveComponentState::SkillList(ListState::new(skills));
        self.trace_active_component(COMPONENT_SKILL_LIST, Some(entry_count));
    }

    /// Shows the models list view without changing history.
    pub fn show_models_list(&mut self, models: Vec<Model>) {
        let entry_count = models.len().saturating_add(1);
        let mut entries = Vec::with_capacity(entry_count);
        entries.push(ModelListEntry::Auto);
        entries.extend(models.into_iter().map(ModelListEntry::Model));
        self.active_component = ActiveComponentState::ModelsList(ListState::new(entries));
        self.trace_active_component(COMPONENT_MODELS_LIST, Some(entry_count));
    }

    /// Shows the providers list view without changing history.
    pub fn show_providers_list(&mut self, providers: Vec<Provider>) {
        let entry_count = providers.len();
        self.active_component = ActiveComponentState::ProvidersList(ListState::new(providers));
        self.trace_active_component(COMPONENT_PROVIDERS_LIST, Some(entry_count));
    }

    /// Shows the usage view without changing history.
    pub fn show_usage(&mut self, usage: UsageState) {
        self.active_component = ActiveComponentState::Usage(usage);
        self.trace_active_component(COMPONENT_USAGE, None);
    }

    /// Shows the tracing list view without changing history.
    pub fn show_tracing_list(&mut self, trace_events: Vec<TraceEvent>) {
        let entry_count = trace_events.len();
        self.active_component = ActiveComponentState::TracingList(ListState::new(trace_events));
        self.trace_active_component(COMPONENT_TRACING_LIST, Some(entry_count));
    }

    /// Shows the sessions list view without changing history.
    pub fn show_sessions_list(&mut self, sessions: Vec<SessionListItem>) {
        let entry_count = sessions.len();
        self.active_component = ActiveComponentState::SessionsList(ListState::new(sessions));
        self.trace_active_component(COMPONENT_SESSIONS_LIST, Some(entry_count));
    }

    /// Applies a router-client message to state.
    pub fn apply_msg(&mut self, msg: Msg) {
        let message = message_kind(&msg);
        tracing::trace!(
            message,
            active_component = self.active_component.kind().to_string(),
            router.state = self.router.kind(),
            history.entries = self.history.len(),
            "applying tui state message"
        );

        match msg {
            Msg::ApprovalPrompt(prompt) => {
                self.execution_turn = Some(ExecutionTurn::Approval(prompt.clone()));
                self.history.push(HistoryEntry::ApprovalRequest(prompt));
            }
            Msg::AssistantTurn(turn) => {
                self.execution_turn = None;
                self.history
                    .push(HistoryEntry::AssistantMessage(turn.message));
            }
            Msg::Error(error) => {
                self.history.push(HistoryEntry::Error(error));
            }
            Msg::Idle => {
                self.router = RouterState::Idle;
                self.execution_turn = None;
            }
            Msg::Interrupted => {
                self.router = RouterState::Interrupted;
                self.execution_turn = None;
            }
            Msg::ModelsList(mut models) => {
                if let Some(awaited_model) = self.take_awaited_model() {
                    if let Some(model_ref) = Self::match_model(&awaited_model, &models) {
                        self.set_preferred_model(model_ref);
                    } else {
                        self.push_history(HistoryEntry::Error(format!(
                            r#"Unknown model: "{awaited_model}". List available models with `/model`"#
                        )));
                    }
                } else {
                    // sort models by (provider, display_name) for consistent ordering
                    models.sort_by(|a, b| {
                        (&a.provider, &a.display_name).cmp(&(&b.provider, &b.display_name))
                    });

                    self.show_models_list(models);
                }
            }
            Msg::Preview(preview) => {
                self.history.push(HistoryEntry::Preview(preview));
            }
            Msg::ProvidersList(providers) => {
                self.show_providers_list(providers);
            }
            Msg::ResumedSession(session) => {
                self.clear_history();
                self.show_console();
                self.history.extend(
                    session
                        .messages
                        .into_iter()
                        .map(|message| HistoryEntry::AssistantMessage(message.content)),
                );
            }
            Msg::RouterStatus(status) => {
                self.history.push(HistoryEntry::Notice(format!(
                    "{ROUTER_NOTICE_PREFIX} {} ({})",
                    status.status, status.version
                )));
            }
            Msg::SessionsList(sessions) => {
                self.show_sessions_list(sessions);
            }
            Msg::StreamedContentChunk(chunk) => {
                self.execution_turn
                    .get_or_insert_with(ExecutionTurn::streaming)
                    .push_content(&chunk);
            }
            Msg::StreamedReasoningChunk(chunk) => {
                self.execution_turn
                    .get_or_insert_with(ExecutionTurn::streaming)
                    .push_reasoning(&chunk);
            }
            Msg::Thinking => {
                self.router = RouterState::Thinking(std::time::Instant::now());
            }
            Msg::ToolCallStarted(tool_call) => {
                self.execution_turn = Some(ExecutionTurn::ToolCall(tool_call.clone()));
                self.history.push(HistoryEntry::ToolCall {
                    name: tool_call.name,
                    input: tool_call.call_id,
                });
            }
            Msg::Trace(trace) => {
                self.show_tracing_list(trace.events.clone());
                self.history
                    .extend(trace.events.into_iter().map(HistoryEntry::Trace));
            }
            Msg::Usage(usage) => {
                self.show_usage(UsageState::new(usage));
            }
        }

        tracing::trace!(
            message,
            active_component = self.active_component.kind().to_string(),
            router.state = self.router.kind(),
            history.entries = self.history.len(),
            "applied tui state message"
        );
    }

    fn trace_active_component(&self, component: &'static str, entry_count: Option<usize>) {
        if let Some(entry_count) = entry_count {
            tracing::debug!(
                component,
                entry.count = entry_count,
                history.entries = self.history.len(),
                "show tui active component"
            );
        } else {
            tracing::debug!(
                component,
                history.entries = self.history.len(),
                "show tui active component"
            );
        }
    }

    fn match_model(model: &str, models: &[Model]) -> Option<ModelReference> {
        models
            .iter()
            .find(|m| {
                let model_ref = format!("{provider}/{id}", provider = m.provider, id = m.id);
                m.id == model || m.display_name == model || model_ref == model
            })
            .map(|m| m.reference.clone())
    }
}

fn message_kind(msg: &Msg) -> &'static str {
    match msg {
        Msg::AssistantTurn(_) => MESSAGE_ASSISTANT_TURN,
        Msg::StreamedContentChunk(_) => MESSAGE_STREAMED_CONTENT_CHUNK,
        Msg::StreamedReasoningChunk(_) => MESSAGE_STREAMED_REASONING_CHUNK,
        Msg::ToolCallStarted(_) => MESSAGE_TOOL_CALL_STARTED,
        Msg::ApprovalPrompt(_) => MESSAGE_APPROVAL_PROMPT,
        Msg::ModelsList(_) => MESSAGE_MODELS_LIST,
        Msg::ProvidersList(_) => MESSAGE_PROVIDERS_LIST,
        Msg::SessionsList(_) => MESSAGE_SESSIONS_LIST,
        Msg::ResumedSession(_) => MESSAGE_RESUMED_SESSION,
        Msg::Usage(_) => MESSAGE_USAGE,
        Msg::Trace(_) => MESSAGE_TRACE,
        Msg::Preview(_) => MESSAGE_PREVIEW,
        Msg::RouterStatus(_) => MESSAGE_ROUTER_STATUS,
        Msg::Error(_) => MESSAGE_ERROR,
        Msg::Idle => MESSAGE_IDLE,
        Msg::Thinking => MESSAGE_THINKING,
        Msg::Interrupted => MESSAGE_INTERRUPTED,
    }
}

#[cfg(test)]
mod tests {
    use smista_sdk::core::api::SessionUsageResponse;
    use smista_sdk::core::usage::Usage;
    use uuid::Uuid;

    use super::*;
    use crate::app::router_client::msg::{
        ApprovalPrompt, PreviewSummary, ResumedSession, RouterStatus, SessionMessage,
        ToolCallStarted, TraceEvent, TraceSummary,
    };

    const ASSISTANT_MESSAGE: &str = "hello";
    const CONTENT_CHUNK: &str = "hello ";
    const ERROR_MESSAGE: &str = "router failed";
    const MODEL_DISPLAY_NAME: &str = "GPT-4.1";
    const MODEL_ID: &str = "gpt-4.1";
    const MODEL_REFERENCE: &str = "openai/gpt-4.1";
    const PREVIEW_MODEL: &str = "gpt-4.1";
    const PREVIEW_PROVIDER: &str = "openai";
    const PREVIEW_TASK_TYPE: &str = "code";
    const PROVIDER_OPENAI: &str = "openai";
    const REASONING_CHUNK: &str = "because ";
    const RESUMED_MESSAGE: &str = "resumed message";
    const RESUMED_ROLE: &str = "assistant";
    const RESUMED_TITLE: &str = "Resumed session";
    const ROUTER_STATUS: &str = "ok";
    const SESSION_TITLE: &str = "Fix resume flow";
    const SESSION_UPDATED_AT: &str = "2026-07-08T10:00:00Z";
    const ROUTER_VERSION: &str = "0.1.0";
    const TOOL_CALL_ID: &str = "tool-call-1";
    const TOOL_NAME: &str = "shell";
    const TRACE_CREATED_AT: &str = "2026-07-08T10:00:00Z";
    const TRACE_EVENT_TYPE: &str = "route";
    const TRACE_MODEL: &str = "gpt-4.1";
    const TRACE_PAYLOAD: &str = "{}";
    const TRACE_TASK_TYPE: &str = "plan";
    const MODELS_LIST_EXPECTED: &str = "models list expected";

    #[test]
    fn list_messages_replace_active_component_without_polluting_history() {
        let mut state = State::default();

        state.apply_msg(Msg::ModelsList(vec![model(MODEL_ID, MODEL_DISPLAY_NAME)]));

        assert!(state.history.is_empty());
        let ActiveComponentState::ModelsList(models) = &state.active_component else {
            panic!("{MODELS_LIST_EXPECTED}");
        };
        assert_eq!(models.entries().len(), 2);
        assert_eq!(models.entries().first(), Some(&ModelListEntry::Auto));
    }

    #[test]
    fn usage_message_replaces_active_component_without_polluting_history() {
        let mut state = State::default();

        state.apply_msg(Msg::Usage(SessionUsageResponse {
            total: Usage::default(),
            by_model: Vec::new(),
            by_task_type: Vec::new(),
        }));

        assert!(state.history.is_empty());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::Usage(_)
        ));
    }

    #[test]
    fn sessions_list_message_replaces_active_component_without_polluting_history() {
        let mut state = State::default();
        let session = SessionListItem {
            id: Uuid::nil(),
            title: Some(SESSION_TITLE.to_owned()),
            scope: Some("project".to_owned()),
            updated_at: SESSION_UPDATED_AT.to_owned(),
        };

        state.apply_msg(Msg::SessionsList(vec![session.clone()]));

        assert!(state.history.is_empty());
        let ActiveComponentState::SessionsList(sessions) = &state.active_component else {
            panic!("sessions list expected");
        };
        assert_eq!(sessions.selected(), Some(&session));
    }

    #[test]
    fn assistant_turn_becomes_transcript_history() {
        let mut state = State::default();

        state.apply_msg(Msg::AssistantTurn(
            crate::app::router_client::msg::AssistantTurn {
                message: ASSISTANT_MESSAGE.to_owned(),
                trace_id: None,
            },
        ));

        assert_eq!(
            state.history,
            vec![HistoryEntry::AssistantMessage(ASSISTANT_MESSAGE.to_owned())]
        );
    }

    #[test]
    fn preferred_model_defaults_to_none_and_can_be_changed() {
        let mut state = State::default();
        assert_eq!(state.preferred_model(), None);

        let model = MODEL_REFERENCE
            .parse::<ModelReference>()
            .expect("model reference parses");
        state.set_preferred_model(model.clone());

        assert_eq!(state.preferred_model(), Some(&model));

        state.clear_preferred_model();

        assert_eq!(state.preferred_model(), None);
    }

    #[test]
    fn prompt_history_keeps_bounded_non_empty_entries() {
        let mut state = State::default();

        state.push_prompt_history("   ".to_owned());
        assert!(state.prompt_history.is_empty());

        for index in 0..PROMPT_HISTORY_LIMIT + 1 {
            state.push_prompt_history(format!("prompt-{index}"));
        }

        assert_eq!(state.prompt_history.len(), PROMPT_HISTORY_LIMIT);
        assert_eq!(
            state.prompt_history.front().map(String::as_str),
            Some("prompt-1")
        );
        assert_eq!(
            state.prompt_history.back().map(String::as_str),
            Some("prompt-100")
        );
    }

    #[test]
    fn prompt_history_navigation_moves_older_newer_and_clears() {
        let mut state = State::default();
        state.push_prompt_history("first".to_owned());
        state.push_prompt_history("second".to_owned());

        assert_eq!(
            state.previous_prompt_history_entry().as_deref(),
            Some("second")
        );
        assert_eq!(
            state.previous_prompt_history_entry().as_deref(),
            Some("first")
        );
        assert_eq!(
            state.next_prompt_history_entry(),
            PromptHistoryNavigation::Entry("second".to_owned())
        );
        assert_eq!(
            state.next_prompt_history_entry(),
            PromptHistoryNavigation::Clear
        );
        assert_eq!(
            state.next_prompt_history_entry(),
            PromptHistoryNavigation::Unchanged
        );
    }

    #[test]
    fn models_list_message_sorts_models_after_auto_entry() {
        let mut state = State::default();

        state.apply_msg(Msg::ModelsList(vec![
            model("z-model", "Z model"),
            model("a-model", "A model"),
        ]));

        let ActiveComponentState::ModelsList(models) = &state.active_component else {
            panic!("{MODELS_LIST_EXPECTED}");
        };
        assert!(matches!(models.entries()[0], ModelListEntry::Auto));
        assert_model_entry(&models.entries()[1], "a-model");
        assert_model_entry(&models.entries()[2], "z-model");
    }

    #[test]
    fn awaited_model_sets_preferred_model_without_showing_list() {
        let mut state = State::default();
        state.set_awaited_model(MODEL_DISPLAY_NAME.to_owned());

        state.apply_msg(Msg::ModelsList(vec![model(MODEL_ID, MODEL_DISPLAY_NAME)]));

        assert_eq!(
            state
                .preferred_model()
                .map(std::string::ToString::to_string)
                .as_deref(),
            Some(MODEL_REFERENCE)
        );
        assert!(state.awaited_model.is_none());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::Console(_)
        ));
        assert!(state.history.is_empty());
    }

    #[test]
    fn awaited_unknown_model_reports_error_without_setting_preference() {
        let mut state = State::default();
        state.set_awaited_model("missing".to_owned());

        state.apply_msg(Msg::ModelsList(vec![model(MODEL_ID, MODEL_DISPLAY_NAME)]));

        assert_eq!(state.preferred_model(), None);
        assert_eq!(
            state.history.last(),
            Some(&HistoryEntry::Error(
                r#"Unknown model: "missing". List available models with `/model`"#.to_owned()
            ))
        );
    }

    #[test]
    fn show_methods_switch_active_component_without_touching_history() {
        let mut state = State::default();
        state.push_history(HistoryEntry::AssistantMessage(ASSISTANT_MESSAGE.to_owned()));

        state.show_skill_list(Vec::new());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::SkillList(_)
        ));

        state.show_models_list(Vec::new());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::ModelsList(_)
        ));

        state.show_providers_list(Vec::new());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::ProvidersList(_)
        ));

        state.show_usage(UsageState::new(SessionUsageResponse {
            total: Usage::default(),
            by_model: Vec::new(),
            by_task_type: Vec::new(),
        }));
        assert!(matches!(
            state.active_component,
            ActiveComponentState::Usage(_)
        ));

        state.show_tracing_list(Vec::new());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::TracingList(_)
        ));

        state.show_sessions_list(Vec::new());
        assert!(matches!(
            state.active_component,
            ActiveComponentState::SessionsList(_)
        ));

        state.show_console();
        assert!(matches!(
            state.active_component,
            ActiveComponentState::Console(_)
        ));

        assert_eq!(
            state.history,
            vec![HistoryEntry::AssistantMessage(ASSISTANT_MESSAGE.to_owned())]
        );
    }

    #[test]
    fn streaming_chunks_update_transient_execution_turn() {
        let mut state = State::default();

        state.apply_msg(Msg::StreamedContentChunk(CONTENT_CHUNK.to_owned()));
        state.apply_msg(Msg::StreamedReasoningChunk(REASONING_CHUNK.to_owned()));

        let Some(ExecutionTurn::Streaming { content, reasoning }) = state.execution_turn else {
            panic!("streaming execution turn expected");
        };
        assert_eq!(content, CONTENT_CHUNK);
        assert_eq!(reasoning, REASONING_CHUNK);
        assert!(state.history.is_empty());
    }

    #[test]
    fn tool_call_and_approval_messages_update_history_and_execution_turn() {
        let mut state = State::default();

        state.apply_msg(Msg::ToolCallStarted(ToolCallStarted {
            call_id: TOOL_CALL_ID.to_owned(),
            name: TOOL_NAME.to_owned(),
        }));

        assert_eq!(
            state.history,
            vec![HistoryEntry::ToolCall {
                name: TOOL_NAME.to_owned(),
                input: TOOL_CALL_ID.to_owned(),
            }]
        );
        assert!(matches!(
            state.execution_turn,
            Some(ExecutionTurn::ToolCall(_))
        ));

        let approval = ApprovalPrompt {
            id: "approval-1".to_owned(),
            title: "Approve command".to_owned(),
            detail: "run shell".to_owned(),
            tool_name: Some("shell".to_owned()),
            wildcard_alias: None,
        };
        state.apply_msg(Msg::ApprovalPrompt(approval.clone()));

        assert_eq!(
            state.history.last(),
            Some(&HistoryEntry::ApprovalRequest(approval))
        );
        assert!(matches!(
            state.execution_turn,
            Some(ExecutionTurn::Approval(_))
        ));
    }

    #[test]
    fn resumed_session_clears_previous_history_and_restores_prompt() {
        let mut state = State::default();
        state.push_history(HistoryEntry::Error(ERROR_MESSAGE.to_owned()));
        state.show_models_list(Vec::new());

        state.apply_msg(Msg::ResumedSession(ResumedSession {
            id: Uuid::nil(),
            title: RESUMED_TITLE.to_owned(),
            messages: vec![SessionMessage {
                role: RESUMED_ROLE.to_owned(),
                content: RESUMED_MESSAGE.to_owned(),
            }],
        }));

        assert!(matches!(
            state.active_component,
            ActiveComponentState::Console(_)
        ));
        assert_eq!(
            state.history,
            vec![HistoryEntry::AssistantMessage(RESUMED_MESSAGE.to_owned())]
        );
    }

    #[test]
    fn trace_message_updates_trace_view_and_history() {
        let mut state = State::default();
        let trace_event = TraceEvent {
            event_type: TRACE_EVENT_TYPE,
            task_type: TRACE_TASK_TYPE,
            provider: PROVIDER_OPENAI.to_owned(),
            model: TRACE_MODEL.to_owned(),
            matched_rule: None,
            created_at: TRACE_CREATED_AT.to_owned(),
            payload: TRACE_PAYLOAD.to_owned(),
        };

        state.apply_msg(Msg::Trace(TraceSummary {
            events: vec![trace_event.clone()],
        }));

        let ActiveComponentState::TracingList(trace_events) = &state.active_component else {
            panic!("tracing list expected");
        };
        assert_eq!(trace_events.selected(), Some(&trace_event));
        assert_eq!(state.history, vec![HistoryEntry::Trace(trace_event)]);
    }

    #[test]
    fn status_preview_error_and_router_lifecycle_messages_update_state() {
        let mut state = State::default();

        state.apply_msg(Msg::Thinking);
        assert_eq!(state.router.kind(), "thinking");

        state.apply_msg(Msg::Preview(PreviewSummary {
            task_type: PREVIEW_TASK_TYPE.to_owned(),
            provider: PREVIEW_PROVIDER.to_owned(),
            model: PREVIEW_MODEL.to_owned(),
            required_permissions: Vec::new(),
        }));
        assert!(matches!(
            state.history.last(),
            Some(HistoryEntry::Preview(_))
        ));

        state.apply_msg(Msg::RouterStatus(RouterStatus {
            status: ROUTER_STATUS.to_owned(),
            version: ROUTER_VERSION.to_owned(),
        }));
        assert_eq!(
            state.history.last(),
            Some(&HistoryEntry::Notice(format!(
                "{ROUTER_NOTICE_PREFIX} {ROUTER_STATUS} ({ROUTER_VERSION})"
            )))
        );

        state.apply_msg(Msg::Error(ERROR_MESSAGE.to_owned()));
        assert_eq!(
            state.history.last(),
            Some(&HistoryEntry::Error(ERROR_MESSAGE.to_owned()))
        );

        state.apply_msg(Msg::Interrupted);
        assert_eq!(state.router.kind(), "interrupted");
        assert!(state.execution_turn.is_none());
        assert_eq!(
            state.history.last(),
            Some(&HistoryEntry::Error(ERROR_MESSAGE.to_owned()))
        );

        state.execution_turn = Some(ExecutionTurn::streaming());
        state.apply_msg(Msg::Idle);

        assert_eq!(state.router.kind(), "idle");
        assert!(state.execution_turn.is_none());
    }

    fn model(id: &str, display_name: &str) -> Model {
        Model {
            reference: format!("{PROVIDER_OPENAI}/{id}")
                .parse::<ModelReference>()
                .expect("model reference parses"),
            provider: PROVIDER_OPENAI.to_owned(),
            id: id.to_owned(),
            display_name: display_name.to_owned(),
            max_context_tokens: 128_000,
            max_output_tokens: Some(16_000),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        }
    }

    fn assert_model_entry(entry: &ModelListEntry, expected_id: &str) {
        let ModelListEntry::Model(model) = entry else {
            panic!("model list entry expected");
        };
        assert_eq!(model.id, expected_id);
    }
}
