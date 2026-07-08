//! TUI State

#![expect(
    dead_code,
    reason = "TUI state is implemented before the renderer consumes every state branch."
)]

mod active_component;
mod history;
mod list;
mod prompt;
mod router;
mod turn;

use smista_sdk::core::model::ModelReference;

pub use self::active_component::{ActiveComponentState, UsageState};
pub use self::history::HistoryEntry;
pub use self::list::ListState;
pub use self::prompt::PromptState;
pub use self::router::RouterState;
pub use self::turn::ExecutionTurn;
use crate::app::router_client::Msg;
use crate::app::router_client::msg::{Model, Provider, SessionListItem, TraceEvent};
use crate::skills::SkillEntry;

const COMPONENT_MODELS_LIST: &str = "models_list";
const COMPONENT_PROMPT: &str = "prompt";
const COMPONENT_PROVIDERS_LIST: &str = "providers_list";
const COMPONENT_SESSIONS_LIST: &str = "sessions_list";
const COMPONENT_SKILL_LIST: &str = "skill_list";
const COMPONENT_TRACING_LIST: &str = "tracing_list";
const COMPONENT_USAGE: &str = "usage";
const INTERRUPTED_NOTICE: &str = "Interrupted. What should the LLM do instead?";
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
const ROUTER_NOTICE_PREFIX: &str = "router";

/// Terminal UI state for the smista-cli user interface.
#[derive(Debug, Default)]
pub struct State {
    /// State of the active component in the TUI.
    ///
    /// On active component change, the previous state is lost.
    pub active_component: ActiveComponentState,
    /// The active execution turn, if any
    pub execution_turn: Option<ExecutionTurn>,
    /// Conversation history
    pub history: Vec<HistoryEntry>,
    /// Preferred model selected by the user.
    pub preferred_model: Option<ModelReference>,
    /// The state of the smista-router execution ()
    pub router: RouterState,
}

impl State {
    /// Creates empty terminal UI state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a renderable history entry.
    pub fn push_history(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        tracing::trace!(
            history.entries = self.history.len(),
            "appended tui history entry"
        );
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

    /// Clears the preferred model selected by the user.
    pub fn clear_preferred_model(&mut self) {
        self.preferred_model = None;
        tracing::debug!(
            preferred_model.present = false,
            "cleared tui preferred model"
        );
    }

    /// Restores the main prompt view without changing history.
    pub fn show_prompt(&mut self) {
        self.active_component = ActiveComponentState::Prompt(PromptState::default());
        self.trace_active_component(COMPONENT_PROMPT, None);
    }

    /// Shows the skill list view without changing history.
    pub fn show_skill_list(&mut self, skills: Vec<SkillEntry>) {
        let entry_count = skills.len();
        self.active_component = ActiveComponentState::SkillList(ListState::new(skills));
        self.trace_active_component(COMPONENT_SKILL_LIST, Some(entry_count));
    }

    /// Shows the models list view without changing history.
    pub fn show_models_list(&mut self, models: Vec<Model>) {
        let entry_count = models.len();
        self.active_component = ActiveComponentState::ModelsList(ListState::new(models));
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
            active_component = self.active_component.kind(),
            router.state = self.router.kind(),
            history.entries = self.history.len(),
            "applying tui state message"
        );

        match msg {
            Msg::AssistantTurn(turn) => {
                self.execution_turn = None;
                self.history
                    .push(HistoryEntry::AssistantMessage(turn.message));
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
            Msg::ToolCallStarted(tool_call) => {
                self.execution_turn = Some(ExecutionTurn::ToolCall(tool_call.clone()));
                self.history.push(HistoryEntry::ToolCall {
                    name: tool_call.name,
                    input: tool_call.call_id,
                });
            }
            Msg::ApprovalPrompt(prompt) => {
                self.execution_turn = Some(ExecutionTurn::Approval(prompt.clone()));
                self.history.push(HistoryEntry::ApprovalRequest(prompt));
            }
            Msg::ModelsList(models) => {
                self.show_models_list(models);
            }
            Msg::ProvidersList(providers) => {
                self.show_providers_list(providers);
            }
            Msg::SessionsList(sessions) => {
                self.show_sessions_list(sessions);
            }
            Msg::ResumedSession(session) => {
                self.clear_history();
                self.show_prompt();
                self.history.extend(
                    session
                        .messages
                        .into_iter()
                        .map(|message| HistoryEntry::AssistantMessage(message.content)),
                );
            }
            Msg::Usage(usage) => {
                self.show_usage(UsageState::new(usage));
            }
            Msg::Trace(trace) => {
                self.show_tracing_list(trace.events.clone());
                self.history
                    .extend(trace.events.into_iter().map(HistoryEntry::Trace));
            }
            Msg::Preview(preview) => {
                self.history.push(HistoryEntry::Preview(preview));
            }
            Msg::RouterStatus(status) => {
                self.history.push(HistoryEntry::Notice(format!(
                    "{ROUTER_NOTICE_PREFIX} {} ({})",
                    status.status, status.version
                )));
            }
            Msg::Error(error) => {
                self.history.push(HistoryEntry::Error(error));
            }
            Msg::Idle => {
                self.router = RouterState::Idle;
                self.execution_turn = None;
            }
            Msg::Thinking => {
                self.router = RouterState::Thinking;
            }
            Msg::Interrupted => {
                self.router = RouterState::Interrupted;
                self.execution_turn = None;
                self.history
                    .push(HistoryEntry::Notice(INTERRUPTED_NOTICE.to_owned()));
            }
        }

        tracing::trace!(
            message,
            active_component = self.active_component.kind(),
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
        let mut state = State::new();

        state.apply_msg(Msg::ModelsList(vec![Model {
            provider: PROVIDER_OPENAI.to_owned(),
            id: MODEL_ID.to_owned(),
            display_name: MODEL_DISPLAY_NAME.to_owned(),
            max_context_tokens: 128_000,
            max_output_tokens: Some(16_000),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        }]));

        assert!(state.history.is_empty());
        let ActiveComponentState::ModelsList(models) = &state.active_component else {
            panic!("{MODELS_LIST_EXPECTED}");
        };
        assert_eq!(models.entries().len(), 1);
    }

    #[test]
    fn usage_message_replaces_active_component_without_polluting_history() {
        let mut state = State::new();

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
    fn assistant_turn_becomes_transcript_history() {
        let mut state = State::new();

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
        let mut state = State::new();
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
    fn show_methods_switch_active_component_without_touching_history() {
        let mut state = State::new();
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

        state.show_prompt();
        assert!(matches!(
            state.active_component,
            ActiveComponentState::Prompt(_)
        ));

        assert_eq!(
            state.history,
            vec![HistoryEntry::AssistantMessage(ASSISTANT_MESSAGE.to_owned())]
        );
    }

    #[test]
    fn streaming_chunks_update_transient_execution_turn() {
        let mut state = State::new();

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
        let mut state = State::new();

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
        let mut state = State::new();
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
            ActiveComponentState::Prompt(_)
        ));
        assert_eq!(
            state.history,
            vec![HistoryEntry::AssistantMessage(RESUMED_MESSAGE.to_owned())]
        );
    }

    #[test]
    fn trace_message_updates_trace_view_and_history() {
        let mut state = State::new();
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
        let mut state = State::new();

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
            Some(&HistoryEntry::Notice(INTERRUPTED_NOTICE.to_owned()))
        );

        state.execution_turn = Some(ExecutionTurn::streaming());
        state.apply_msg(Msg::Idle);

        assert_eq!(state.router.kind(), "idle");
        assert!(state.execution_turn.is_none());
    }
}
