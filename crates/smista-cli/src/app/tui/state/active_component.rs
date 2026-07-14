//! Active replacement view state for the TUI.

use std::fmt;

use smista_sdk::core::api::SessionUsageResponse;

use super::ModelListEntry;
use super::list::ListState;
use crate::app::router_client::msg::{Provider, SessionListItem};
use crate::app::tui::state::console::ConsoleState;
use crate::skills::SkillEntry;

const COMPONENT_CONSOLE: &str = "console";
const COMPONENT_MODELS_LIST: &str = "models_list";
const COMPONENT_PROVIDERS_LIST: &str = "providers_list";
const COMPONENT_SESSIONS_LIST: &str = "sessions_list";
const COMPONENT_SKILL_LIST: &str = "skill_list";
const COMPONENT_USAGE: &str = "usage";

/// The state of the active component in the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveComponentState {
    /// Main console view.
    Console(ConsoleState),
    /// Models list view.
    ModelsList(ListState<ModelListEntry>),
    /// Providers list view.
    ProvidersList(ListState<Provider>),
    /// Sessions list view.
    SessionsList(ListState<SessionListItem>),
    /// Skill list view.
    SkillList(ListState<(String, SkillEntry)>),
    /// Usage information view.
    Usage(UsageState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveComponentKind {
    Console,
    ModelsList,
    ProvidersList,
    SessionsList,
    SkillList,
    Usage,
}

impl fmt::Display for ActiveComponentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind_str = match self {
            Self::Console => COMPONENT_CONSOLE,
            Self::SkillList => COMPONENT_SKILL_LIST,
            Self::ModelsList => COMPONENT_MODELS_LIST,
            Self::ProvidersList => COMPONENT_PROVIDERS_LIST,
            Self::Usage => COMPONENT_USAGE,
            Self::SessionsList => COMPONENT_SESSIONS_LIST,
        };
        write!(f, "{}", kind_str)
    }
}

impl ActiveComponentState {
    /// Returns a stable label for tracing and diagnostics.
    #[must_use]
    pub fn kind(&self) -> ActiveComponentKind {
        match self {
            Self::Console(_) => ActiveComponentKind::Console,
            Self::SkillList(_) => ActiveComponentKind::SkillList,
            Self::ModelsList(_) => ActiveComponentKind::ModelsList,
            Self::ProvidersList(_) => ActiveComponentKind::ProvidersList,
            Self::Usage(_) => ActiveComponentKind::Usage,
            Self::SessionsList(_) => ActiveComponentKind::SessionsList,
        }
    }

    /// Returns the console state when the console view is active.
    #[must_use]
    pub fn console(&self) -> Option<&ConsoleState> {
        match self {
            Self::Console(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable console state when the console view is active.
    #[must_use]
    pub fn console_mut(&mut self) -> Option<&mut ConsoleState> {
        match self {
            Self::Console(state) => Some(state),
            _ => None,
        }
    }

    /// Pushes a character into the console when the console view is active.
    pub fn push_console(&mut self, char: char) {
        if let Self::Console(console) = self {
            console.prompt.push(char);
        }
    }

    /// Returns the skill list state when active.
    #[must_use]
    pub fn skill_list(&self) -> Option<&ListState<(String, SkillEntry)>> {
        match self {
            Self::SkillList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable skill list state when active.
    #[must_use]
    pub fn skill_list_mut(&mut self) -> Option<&mut ListState<(String, SkillEntry)>> {
        match self {
            Self::SkillList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the models list state when active.
    #[must_use]
    pub fn models_list(&self) -> Option<&ListState<ModelListEntry>> {
        match self {
            Self::ModelsList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable models list state when active.
    #[must_use]
    pub fn models_list_mut(&mut self) -> Option<&mut ListState<ModelListEntry>> {
        match self {
            Self::ModelsList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the providers list state when active.
    #[must_use]
    pub fn providers_list(&self) -> Option<&ListState<Provider>> {
        match self {
            Self::ProvidersList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable providers list state when active.
    #[must_use]
    pub fn providers_list_mut(&mut self) -> Option<&mut ListState<Provider>> {
        match self {
            Self::ProvidersList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the usage state when active.
    #[must_use]
    pub fn usage(&self) -> Option<&UsageState> {
        match self {
            Self::Usage(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable usage state when active.
    #[must_use]
    pub fn usage_mut(&mut self) -> Option<&mut UsageState> {
        match self {
            Self::Usage(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the sessions list state when active.
    #[must_use]
    pub fn sessions_list(&self) -> Option<&ListState<SessionListItem>> {
        match self {
            Self::SessionsList(state) => Some(state),
            _ => None,
        }
    }

    /// Returns the mutable sessions list state when active.
    #[must_use]
    pub fn sessions_list_mut(&mut self) -> Option<&mut ListState<SessionListItem>> {
        match self {
            Self::SessionsList(state) => Some(state),
            _ => None,
        }
    }
}

impl Default for ActiveComponentState {
    fn default() -> Self {
        Self::Console(ConsoleState::default())
    }
}

/// State for the usage information view.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageState {
    usage: SessionUsageResponse,
}

impl UsageState {
    /// Creates usage view state.
    #[must_use]
    pub fn new(usage: SessionUsageResponse) -> Self {
        Self { usage }
    }

    /// Returns usage information.
    #[must_use]
    pub fn usage(&self) -> &SessionUsageResponse {
        &self.usage
    }

    /// Replaces usage information.
    pub fn replace(&mut self, usage: SessionUsageResponse) {
        self.usage = usage;
    }
}

#[cfg(test)]
mod tests {
    use smista_sdk::core::api::SessionUsageResponse;
    use smista_sdk::core::model::{ModelReference, Provider as CoreProvider};
    use smista_sdk::core::usage::Usage;
    use uuid::Uuid;

    use super::*;
    use crate::app::router_client::msg::Model;

    const MODEL_DISPLAY_NAME: &str = "GPT-4.1";
    const MODEL_ID: &str = "gpt-4.1";
    const PROVIDER_NAME: &str = "openai";
    const SESSION_UPDATED_AT: &str = "2026-07-08T10:00:00Z";
    fn usage_response(input_tokens: u64) -> SessionUsageResponse {
        SessionUsageResponse {
            total: Usage {
                input_tokens: Some(input_tokens),
                ..Default::default()
            },
            by_model: Vec::new(),
            by_task_type: Vec::new(),
        }
    }

    fn model() -> Model {
        Model {
            reference: ModelReference {
                provider: CoreProvider::OpenAI,
                model: MODEL_ID.to_owned(),
            },
            provider: PROVIDER_NAME.to_owned(),
            id: MODEL_ID.to_owned(),
            display_name: MODEL_DISPLAY_NAME.to_owned(),
            max_context_tokens: 128_000,
            max_output_tokens: Some(16_000),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        }
    }

    #[test]
    fn default_component_is_prompt() {
        let state = ActiveComponentState::default();

        assert_eq!(state.kind(), ActiveComponentKind::Console);
        assert!(state.console().is_some());
    }

    #[test]
    fn console_accessors_only_match_console_component() {
        let mut state = ActiveComponentState::Console(ConsoleState::default());

        state.push_console('h');
        state.console_mut().expect("console state").prompt.push('i');

        let prompt = &state.console().expect("console state").prompt;
        assert_eq!(prompt.input(), "hi");
        assert_eq!(prompt.cursor_position(), 2);

        let mut models =
            ActiveComponentState::ModelsList(ListState::new(vec![ModelListEntry::Model(model())]));
        models.push_console('x');

        assert!(models.console().is_none());
        assert!(models.console_mut().is_none());
    }

    #[test]
    fn list_accessors_only_match_their_component() {
        let mut skills = ActiveComponentState::SkillList(ListState::default());
        assert_eq!(skills.kind(), ActiveComponentKind::SkillList);
        assert!(skills.skill_list().expect("skill list").is_empty());
        skills.skill_list_mut().expect("skill list").next();

        let mut models =
            ActiveComponentState::ModelsList(ListState::new(vec![ModelListEntry::Model(model())]));
        assert_eq!(models.kind(), ActiveComponentKind::ModelsList);
        assert_eq!(
            models.models_list().expect("models list").entries().len(),
            1
        );
        models.models_list_mut().expect("models list").next();

        let mut providers = ActiveComponentState::ProvidersList(ListState::new(vec![
            crate::app::tui::state::Provider {
                name: PROVIDER_NAME.to_owned(),
                local: false,
            },
        ]));
        assert_eq!(providers.kind(), ActiveComponentKind::ProvidersList);
        assert_eq!(
            providers
                .providers_list()
                .expect("providers list")
                .selected()
                .expect("provider")
                .name,
            PROVIDER_NAME
        );
        providers
            .providers_list_mut()
            .expect("providers list")
            .next();

        let mut sessions =
            ActiveComponentState::SessionsList(ListState::new(vec![SessionListItem {
                id: Uuid::nil(),
                title: None,
                scope: None,
                updated_at: SESSION_UPDATED_AT.to_owned(),
            }]));
        assert_eq!(sessions.kind(), ActiveComponentKind::SessionsList);
        assert_eq!(
            sessions
                .sessions_list()
                .expect("sessions list")
                .selected()
                .expect("session")
                .id,
            Uuid::nil()
        );
        sessions.sessions_list_mut().expect("sessions list").next();

        assert!(skills.models_list().is_none());
        assert!(models.providers_list().is_none());
        assert!(sessions.skill_list().is_none());
    }

    #[test]
    fn usage_state_can_be_read_and_replaced() {
        let mut state = ActiveComponentState::Usage(UsageState::new(usage_response(10)));

        assert_eq!(state.kind(), ActiveComponentKind::Usage);
        assert_eq!(
            state
                .usage()
                .expect("usage state")
                .usage()
                .total
                .input_tokens,
            Some(10)
        );

        state
            .usage_mut()
            .expect("usage state")
            .replace(usage_response(20));

        assert_eq!(
            state
                .usage()
                .expect("usage state")
                .usage()
                .total
                .input_tokens,
            Some(20)
        );

        assert!(
            ActiveComponentState::Console(ConsoleState::default())
                .usage()
                .is_none()
        );
    }
}
