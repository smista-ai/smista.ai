use std::collections::HashSet;

use uuid::Uuid;

use super::Tui;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::Cmd;
use crate::app::router_client::cmd::{
    ApprovalDecision, ApprovalOutcome, ApprovalScope, ContinueExecution,
};
use crate::app::router_client::msg::ApprovalPrompt;
use crate::app::tui::state::{
    ActiveComponentKind, ActiveComponentState, Command, ExecutionTurn, HistoryEntry, PromptState,
};

const EDIT_FILE_TOOL: &str = "edit_file";
const WRITE_FILE_TOOL: &str = "write_file";

impl<B> Tui<B>
where
    B: ratatui::backend::Backend,
{
    /// Handles one input event and optionally produces a router command.
    ///
    /// Returns `Some(Cmd)` if a command is produced and should be sent to the router, or `None` if no command is produced.
    pub(in crate::app::tui) fn on_input(&mut self, event: InputEvent) -> Option<Cmd> {
        match self.state.active_component.kind() {
            ActiveComponentKind::Console if self.state.execution_turn.is_some() => {
                self.handle_input_on_execution_turn(event)
            }
            ActiveComponentKind::Console => self.handle_input_on_console(event),
            ActiveComponentKind::ModelsList => self.handle_input_on_list(event, Self::set_model),
            ActiveComponentKind::Usage => self.handle_input_on_usage(event),
            ActiveComponentKind::SkillList
            | ActiveComponentKind::LogsList
            | ActiveComponentKind::ProvidersList
            | ActiveComponentKind::TracingList => self.handle_input_on_list(event, |tui| {
                tui.state.show_console();
                None
            }),
            ActiveComponentKind::SessionsList => {
                self.handle_input_on_list(event, Self::resume_selected_session)
            }
        }
    }

    fn handle_input_on_console(&mut self, event: InputEvent) -> Option<Cmd> {
        let Some(console) = self.state.active_component.console_mut() else {
            tracing::warn!("active component is not console, cannot handle input event");
            return None;
        };

        match event {
            InputEvent::Backspace => {
                console.prompt.backspace();
                None
            }
            InputEvent::Newline => {
                console.prompt.push('\n');
                None
            }
            InputEvent::Enter => match &console.prompt {
                PromptState::Empty => {
                    tracing::debug!("input event is enter, but prompt input is empty, ignoring");
                    None
                }
                PromptState::Text(input) => {
                    let prompt = input.text().trim().to_string();
                    console.prompt.clear();

                    if prompt.is_empty() {
                        tracing::debug!(
                            "input event is enter, but prompt input is empty, ignoring"
                        );
                        None
                    } else {
                        tracing::debug!(
                            "input event is enter, pushing prompt to history and producing execute command"
                        );
                        self.state
                            .push_history(HistoryEntry::UserMessage(prompt.clone()));
                        Some(Cmd::Execute {
                            prompt,
                            files: HashSet::default(),
                            plan: false,
                            explicit_model: self.state.preferred_model.clone(),
                        })
                    }
                }
                PromptState::Command(command) => {
                    let (command, args) = command.resolved();
                    if !matches!(command, Command::Unresolved(_)) {
                        // clear console only if command is resolved
                        console.prompt.clear();
                    }

                    self.handle_command(command, args)
                }
            },
            InputEvent::Tab => {
                console.prompt.accept_suggestion();

                None
            }
            InputEvent::Paste(content) => {
                console.prompt.push_str(&content);

                None
            }
            InputEvent::Delete => {
                console.prompt.delete();

                None
            }
            InputEvent::Left => {
                console.prompt.move_left();

                None
            }
            InputEvent::Right => {
                if !console.prompt.accept_suggestion() {
                    console.prompt.move_right();
                }

                None
            }
            InputEvent::Home => {
                console.prompt.move_home();

                None
            }
            InputEvent::End => {
                console.prompt.move_end();

                None
            }
            InputEvent::Up => {
                console.prompt.move_up();

                None
            }
            InputEvent::Down => {
                console.prompt.move_down();

                None
            }
            InputEvent::Interrupt | InputEvent::Escape => self.handle_interrupt(),
            InputEvent::Char(char) => {
                tracing::debug!("input event is character, pushing to prompt input state");
                console.prompt.push(char);

                None
            }
            _ => {
                tracing::debug!("input event is not handled by the TUI scaffold");

                None
            }
        }
    }

    fn handle_command(&mut self, command: Command, args: Vec<String>) -> Option<Cmd> {
        match command {
            Command::Resume => self.handle_resume_command(&args),
            Command::Quit => {
                tracing::debug!("input event is quit command, producing exit command");
                self.context.exit.cancel();

                None
            }
            Command::Unresolved(unresolved) => {
                tracing::debug!(
                    "input event is unresolved '{unresolved}' command, producing execute command"
                );
                self.state
                    .push_history(HistoryEntry::Error(format!("Unknown command {unresolved}")));

                None
            }
        }
    }

    fn handle_resume_command(&mut self, args: &[String]) -> Option<Cmd> {
        match args {
            [] => {
                tracing::debug!("input event is resume command, listing sessions");

                Some(Cmd::ListSessions)
            }
            [session_id] => match Uuid::parse_str(session_id) {
                Ok(session_id) => {
                    tracing::debug!(%session_id, "input event is resume command with session id");

                    Some(Cmd::ResumeSession(session_id))
                }
                Err(_) => {
                    tracing::debug!(
                        session_id,
                        "input event is resume command with invalid session id"
                    );
                    self.state.push_history(HistoryEntry::Error(format!(
                        "Invalid session id {session_id}"
                    )));

                    None
                }
            },
            _ => {
                tracing::debug!(
                    args.count = args.len(),
                    "input event is resume command with invalid argument count"
                );
                self.state.push_history(HistoryEntry::Error(
                    "Expected at most one session id after /resume".to_owned(),
                ));

                None
            }
        }
    }

    fn handle_input_on_execution_turn(&mut self, event: InputEvent) -> Option<Cmd> {
        let Some(approval) = self.active_approval_prompt().cloned() else {
            return match event {
                InputEvent::Escape | InputEvent::Interrupt => self.handle_interrupt(),
                _ => None,
            };
        };

        match event {
            InputEvent::Up => {
                self.move_approval_cursor_up(&approval);
                None
            }
            InputEvent::Down => {
                self.move_approval_cursor_down(&approval);
                None
            }
            InputEvent::Enter => Some(self.selected_approval_command(&approval)),
            InputEvent::Char('1' | 'y') => {
                Some(approval_command(&approval, ApprovalOption::ApproveOnce))
            }
            InputEvent::Char('2' | 'a') if approval_allows_session_acceptance(&approval) => Some(
                approval_command(&approval, ApprovalOption::ApproveAlwaysForSession),
            ),
            InputEvent::Char('3' | 'n') => {
                Some(approval_command(&approval, ApprovalOption::Reject))
            }
            InputEvent::Escape | InputEvent::Interrupt => self.handle_interrupt(),
            _ => None,
        }
    }

    fn handle_input_on_list<F>(&mut self, event: InputEvent, on_enter: F) -> Option<Cmd>
    where
        F: FnOnce(&mut Self) -> Option<Cmd>,
    {
        match event {
            InputEvent::Up => self.previous_active_list_entry(),
            InputEvent::Down => self.next_active_list_entry(),
            InputEvent::Home => self.first_active_list_entry(),
            InputEvent::End => self.last_active_list_entry(),
            InputEvent::PageUp => self.page_previous_active_list_entry(),
            InputEvent::PageDown => self.page_next_active_list_entry(),
            InputEvent::Enter => return on_enter(self),
            InputEvent::Escape => self.state.show_console(),
            InputEvent::Interrupt => self.context.exit.cancel(),
            _ => {}
        }

        None
    }

    fn handle_input_on_usage(&mut self, event: InputEvent) -> Option<Cmd> {
        match event {
            InputEvent::Enter | InputEvent::Escape => self.state.show_console(),
            InputEvent::Interrupt => self.context.exit.cancel(),
            _ => {}
        }

        None
    }

    fn previous_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.previous(),
            ActiveComponentState::ModelsList(state) => state.previous(),
            ActiveComponentState::ProvidersList(state) => state.previous(),
            ActiveComponentState::SessionsList(state) => state.previous(),
            ActiveComponentState::SkillList(state) => state.previous(),
            ActiveComponentState::TracingList(state) => state.previous(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn next_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.next(),
            ActiveComponentState::ModelsList(state) => state.next(),
            ActiveComponentState::ProvidersList(state) => state.next(),
            ActiveComponentState::SessionsList(state) => state.next(),
            ActiveComponentState::SkillList(state) => state.next(),
            ActiveComponentState::TracingList(state) => state.next(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn first_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.first(),
            ActiveComponentState::ModelsList(state) => state.first(),
            ActiveComponentState::ProvidersList(state) => state.first(),
            ActiveComponentState::SessionsList(state) => state.first(),
            ActiveComponentState::SkillList(state) => state.first(),
            ActiveComponentState::TracingList(state) => state.first(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn last_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.last(),
            ActiveComponentState::ModelsList(state) => state.last(),
            ActiveComponentState::ProvidersList(state) => state.last(),
            ActiveComponentState::SessionsList(state) => state.last(),
            ActiveComponentState::SkillList(state) => state.last(),
            ActiveComponentState::TracingList(state) => state.last(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn page_previous_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.page_previous(),
            ActiveComponentState::ModelsList(state) => state.page_previous(),
            ActiveComponentState::ProvidersList(state) => state.page_previous(),
            ActiveComponentState::SessionsList(state) => state.page_previous(),
            ActiveComponentState::SkillList(state) => state.page_previous(),
            ActiveComponentState::TracingList(state) => state.page_previous(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn page_next_active_list_entry(&mut self) {
        match &mut self.state.active_component {
            ActiveComponentState::LogsList(state) => state.page_next(),
            ActiveComponentState::ModelsList(state) => state.page_next(),
            ActiveComponentState::ProvidersList(state) => state.page_next(),
            ActiveComponentState::SessionsList(state) => state.page_next(),
            ActiveComponentState::SkillList(state) => state.page_next(),
            ActiveComponentState::TracingList(state) => state.page_next(),
            ActiveComponentState::Console(_) | ActiveComponentState::Usage(_) => {}
        }
    }

    fn active_approval_prompt(&self) -> Option<&ApprovalPrompt> {
        match self.state.execution_turn.as_ref() {
            Some(ExecutionTurn::Approval(prompt)) => Some(prompt),
            _ => None,
        }
    }

    fn move_approval_cursor_up(&mut self, prompt: &ApprovalPrompt) {
        let option_count = approval_option_count(prompt);
        if let Some(console) = self.state.active_component.console_mut() {
            console.previous_approval_option(option_count);
        }
    }

    fn move_approval_cursor_down(&mut self, prompt: &ApprovalPrompt) {
        let option_count = approval_option_count(prompt);
        if let Some(console) = self.state.active_component.console_mut() {
            console.next_approval_option(option_count);
        }
    }

    fn selected_approval_command(&mut self, prompt: &ApprovalPrompt) -> Cmd {
        let option_count = approval_option_count(prompt);
        let index = self
            .state
            .active_component
            .console()
            .map(|console| console.approval_option_index(option_count))
            .unwrap_or_default();

        approval_command(prompt, approval_option(prompt, index))
    }

    fn handle_interrupt(&mut self) -> Option<Cmd> {
        if self.state.execution_turn.is_some()
            || matches!(
                self.state.router,
                crate::app::tui::state::RouterState::Thinking(_)
            )
        {
            tracing::debug!(
                "input event is interrupt, producing continue command to break execution"
            );
            Some(Cmd::Continue(ContinueExecution::Break))
        } else {
            tracing::debug!("input event is interrupt, but router is not thinking, exiting");
            self.context.exit.cancel();
            None
        }
    }

    fn set_selected_model_as_preferred(&mut self) {
        let Some((provider, model)) = self
            .state
            .active_component
            .models_list()
            .and_then(|list| list.selected())
            .map(|model| (model.provider.clone(), model.id.clone()))
        else {
            return;
        };

        match provider.parse() {
            Ok(provider) => {
                self.state
                    .set_preferred_model(smista_sdk::core::model::ModelReference {
                        provider,
                        model,
                    });
            }
            Err(err) => {
                tracing::warn!(
                    model.provider = %provider,
                    model.id = %model,
                    error.message = %err,
                    "selected model has invalid provider"
                );
            }
        }
    }

    fn set_model(&mut self) -> Option<Cmd> {
        self.set_selected_model_as_preferred();
        self.state.show_console();

        None
    }

    fn resume_selected_session(&mut self) -> Option<Cmd> {
        let selected_session_id = self
            .state
            .active_component
            .sessions_list()
            .and_then(|list| list.selected())
            .map(|session| session.id);

        self.state.show_console();
        selected_session_id.map(Cmd::ResumeSession)
    }
}

fn approval_command(prompt: &ApprovalPrompt, option: ApprovalOption) -> Cmd {
    Cmd::Continue(ContinueExecution::ApprovalDecisions {
        decisions: vec![approval_decision_for_option(prompt, option)],
    })
}

fn approval_decision_for_option(
    prompt: &ApprovalPrompt,
    option: ApprovalOption,
) -> ApprovalDecision {
    match option {
        ApprovalOption::ApproveOnce => ApprovalDecision {
            id: prompt.id.clone(),
            outcome: ApprovalOutcome::Approved,
            scope: ApprovalScope::Once,
            reason: None,
        },
        ApprovalOption::ApproveAlwaysForSession => ApprovalDecision {
            id: prompt.id.clone(),
            outcome: ApprovalOutcome::Approved,
            scope: ApprovalScope::AlwaysForSession,
            reason: None,
        },
        ApprovalOption::Reject => ApprovalDecision {
            id: prompt.id.clone(),
            outcome: ApprovalOutcome::Rejected,
            scope: ApprovalScope::Once,
            reason: None,
        },
    }
}

fn approval_option(prompt: &ApprovalPrompt, index: usize) -> ApprovalOption {
    if approval_allows_session_acceptance(prompt) {
        return match index {
            0 => ApprovalOption::ApproveOnce,
            1 => ApprovalOption::ApproveAlwaysForSession,
            _ => ApprovalOption::Reject,
        };
    }

    match index {
        0 => ApprovalOption::ApproveOnce,
        _ => ApprovalOption::Reject,
    }
}

fn approval_option_count(prompt: &ApprovalPrompt) -> usize {
    if approval_allows_session_acceptance(prompt) {
        3
    } else {
        2
    }
}

fn approval_allows_session_acceptance(prompt: &ApprovalPrompt) -> bool {
    prompt.tool_name.as_deref().is_some_and(tool_changes_files) || prompt.wildcard_alias.is_some()
}

fn tool_changes_files(name: &str) -> bool {
    matches!(name, WRITE_FILE_TOOL | EDIT_FILE_TOOL)
}

enum ApprovalOption {
    ApproveOnce,
    ApproveAlwaysForSession,
    Reject,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use ratatui::backend::TestBackend;
    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use smista_sdk::core::api::SessionUsageResponse;
    use smista_sdk::core::usage::Usage;
    use tokio_util::sync::CancellationToken;
    use url::Url;
    use uuid::Uuid;

    use super::*;
    use crate::app::AppContext;
    use crate::app::router_client::msg::{Model, Provider, SessionListItem};
    use crate::app::tui::state::{ActiveComponentState, RouterState, UsageState};
    use crate::config::Config;
    use crate::credentials::{CredentialBackend, CredentialsStorage, E2eeKeysCredentials};
    use crate::skills::SkillStore;

    const MODEL_DISPLAY_NAME: &str = "GPT-4.1";
    const MODEL_ID: &str = "gpt-4.1";
    const APPROVAL_ID: &str = "approval-1";
    const PROVIDER_OPENAI: &str = "openai";
    const SESSION_UPDATED_AT: &str = "2026-07-08T10:00:00Z";

    fn app_context(exit: CancellationToken) -> AppContext {
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
            .expect("test credentials storage builds");
        assert_eq!(credentials.backend(), CredentialBackend::File);
        let credentials = Arc::new(credentials);
        let router_client = ReqwestClient::new(RouterClientConfig::new(
            Url::parse("http://127.0.0.1:9").expect("test URL parses"),
        ))
        .expect("test router client builds");

        AppContext {
            config: Arc::new(Config::default()),
            cwd: cwd.clone(),
            e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
            exit,
            router_client: Arc::new(router_client),
            skills_store: Arc::new(SkillStore::discover(&cwd)),
        }
    }

    fn model(id: &str) -> Model {
        Model {
            provider: PROVIDER_OPENAI.to_owned(),
            id: id.to_owned(),
            display_name: MODEL_DISPLAY_NAME.to_owned(),
            max_context_tokens: 128_000,
            max_output_tokens: Some(16_000),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        }
    }

    fn approval_prompt() -> ApprovalPrompt {
        ApprovalPrompt {
            id: APPROVAL_ID.to_owned(),
            title: "Approve command".to_owned(),
            detail: "run shell command".to_owned(),
            tool_name: None,
            wildcard_alias: Some("git status *".to_owned()),
        }
    }

    fn approval_prompt_without_session_acceptance() -> ApprovalPrompt {
        ApprovalPrompt {
            wildcard_alias: None,
            ..approval_prompt()
        }
    }

    fn usage_state() -> UsageState {
        UsageState::new(SessionUsageResponse {
            total: Usage {
                input_tokens: Some(1),
                ..Default::default()
            },
            by_model: Vec::new(),
            by_task_type: Vec::new(),
        })
    }

    fn session(id: Uuid, title: &str) -> SessionListItem {
        SessionListItem {
            id,
            title: Some(title.to_owned()),
            scope: None,
            updated_at: SESSION_UPDATED_AT.to_owned(),
        }
    }

    #[test]
    fn right_accepts_active_command_suggestion() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Char('/'));
        tui.on_input(InputEvent::Char('q'));
        tui.on_input(InputEvent::Right);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.input(), "/quit");
    }

    #[test]
    fn tab_accepts_active_command_suggestion() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Char('/'));
        tui.on_input(InputEvent::Char('q'));
        tui.on_input(InputEvent::Tab);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.input(), "/quit");
    }

    #[test]
    fn right_moves_cursor_inside_command_word() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Char('/'));
        tui.on_input(InputEvent::Char('q'));
        tui.on_input(InputEvent::Char('u'));
        tui.on_input(InputEvent::Left);
        tui.on_input(InputEvent::Right);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.input(), "/qu");
        assert_eq!(console.prompt.cursor_position(), 3);
    }

    #[test]
    fn home_and_end_move_console_prompt_cursor() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Char('a'));
        tui.on_input(InputEvent::Char('b'));
        tui.on_input(InputEvent::Char('c'));
        tui.on_input(InputEvent::Home);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.cursor_position(), 0);

        tui.on_input(InputEvent::End);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.cursor_position(), 3);
    }

    #[test]
    fn list_arrows_move_selection() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_providers_list(vec![
            Provider {
                name: "first".to_owned(),
                local: false,
            },
            Provider {
                name: "second".to_owned(),
                local: true,
            },
        ]);

        tui.on_input(InputEvent::Down);
        assert_eq!(
            tui.state
                .active_component
                .providers_list()
                .expect("providers list is active")
                .current_index(),
            1
        );

        tui.on_input(InputEvent::Up);
        assert_eq!(
            tui.state
                .active_component
                .providers_list()
                .expect("providers list is active")
                .current_index(),
            0
        );
    }

    #[test]
    fn list_home_end_and_page_keys_move_selection() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        let providers = (0..20)
            .map(|index| Provider {
                name: format!("provider-{index}"),
                local: false,
            })
            .collect();
        tui.state.show_providers_list(providers);

        tui.on_input(InputEvent::End);
        assert_eq!(selected_provider_index(&tui), 19);

        tui.on_input(InputEvent::PageUp);
        assert_eq!(selected_provider_index(&tui), 11);

        tui.on_input(InputEvent::Home);
        assert_eq!(selected_provider_index(&tui), 0);

        tui.on_input(InputEvent::PageDown);
        assert_eq!(selected_provider_index(&tui), 8);

        tui.on_input(InputEvent::PageDown);
        tui.on_input(InputEvent::PageDown);
        assert_eq!(selected_provider_index(&tui), 19);
    }

    #[test]
    fn approval_turn_input_does_not_edit_prompt() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::Approval(approval_prompt()));

        tui.on_input(InputEvent::Char('x'));

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.input(), "");
    }

    #[test]
    fn approval_turn_up_and_down_move_selected_option() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::Approval(approval_prompt()));

        tui.on_input(InputEvent::Down);
        assert_eq!(selected_approval_index(&tui), 1);

        tui.on_input(InputEvent::Up);
        assert_eq!(selected_approval_index(&tui), 0);
    }

    #[test]
    fn approval_turn_enter_sends_selected_decision() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::Approval(approval_prompt()));
        tui.on_input(InputEvent::Down);

        let cmd = tui
            .on_input(InputEvent::Enter)
            .expect("approval enter produces a command");

        let Cmd::Continue(ContinueExecution::ApprovalDecisions { decisions }) = cmd else {
            panic!("approval decision command expected");
        };
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, APPROVAL_ID);
        assert_eq!(decisions[0].outcome, ApprovalOutcome::Approved);
        assert_eq!(decisions[0].scope, ApprovalScope::AlwaysForSession);
    }

    #[test]
    fn approval_turn_yes_hotkeys_approve_once() {
        for event in [InputEvent::Char('1'), InputEvent::Char('y')] {
            let cmd = approval_cmd_for_input(event, approval_prompt())
                .expect("approval yes hotkey produces a command");

            assert_approval_decision(cmd, ApprovalOutcome::Approved, ApprovalScope::Once);
        }
    }

    #[test]
    fn approval_turn_always_hotkeys_approve_for_session() {
        for event in [InputEvent::Char('2'), InputEvent::Char('a')] {
            let cmd = approval_cmd_for_input(event, approval_prompt())
                .expect("approval always hotkey produces a command");

            assert_approval_decision(
                cmd,
                ApprovalOutcome::Approved,
                ApprovalScope::AlwaysForSession,
            );
        }
    }

    #[test]
    fn approval_turn_no_hotkeys_reject_once() {
        for event in [InputEvent::Char('3'), InputEvent::Char('n')] {
            let cmd = approval_cmd_for_input(event, approval_prompt())
                .expect("approval no hotkey produces a command");

            assert_approval_decision(cmd, ApprovalOutcome::Rejected, ApprovalScope::Once);
        }
    }

    #[test]
    fn approval_turn_always_hotkeys_require_session_option() {
        for event in [InputEvent::Char('2'), InputEvent::Char('a')] {
            let cmd = approval_cmd_for_input(event, approval_prompt_without_session_acceptance());

            assert_eq!(cmd, None);
        }
    }

    #[test]
    fn active_turn_escape_sends_break() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::Approval(approval_prompt()));

        let cmd = tui
            .on_input(InputEvent::Escape)
            .expect("active turn escape produces a command");

        assert_eq!(cmd, Cmd::Continue(ContinueExecution::Break));
    }

    #[test]
    fn non_approval_execution_turn_ignores_navigation_and_escape_breaks() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::streaming());

        assert_eq!(tui.on_input(InputEvent::Up), None);

        let cmd = tui
            .on_input(InputEvent::Escape)
            .expect("active turn escape produces a command");

        assert_eq!(cmd, Cmd::Continue(ContinueExecution::Break));
    }

    #[test]
    fn interrupt_breaks_while_router_is_thinking() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.router = RouterState::Thinking(Instant::now());

        let cmd = tui
            .on_input(InputEvent::Interrupt)
            .expect("thinking interrupt produces a command");

        assert_eq!(cmd, Cmd::Continue(ContinueExecution::Break));
    }

    #[test]
    fn escape_restores_console_from_list() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_providers_list(Vec::new());

        tui.on_input(InputEvent::Escape);

        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));
    }

    #[test]
    fn interrupt_on_list_exits_application() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit.clone()));
        tui.state.show_providers_list(Vec::new());

        tui.on_input(InputEvent::Interrupt);

        assert!(exit.is_cancelled());
    }

    #[test]
    fn handle_command_resume_lists_sessions() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Resume, Vec::new())
            .expect("resume command produces command");

        assert_eq!(cmd, Cmd::ListSessions);
    }

    #[test]
    fn handle_command_resume_with_session_id_resumes_session() {
        let session_id = Uuid::from_u128(42);
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Resume, vec![session_id.to_string()])
            .expect("resume command with id produces command");

        assert_eq!(cmd, Cmd::ResumeSession(session_id));
    }

    #[test]
    fn handle_command_resume_with_invalid_session_id_reports_error() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        assert_eq!(
            tui.handle_command(Command::Resume, vec!["not-a-session-id".to_owned()]),
            None
        );

        assert_eq!(
            tui.state.history.last(),
            Some(&HistoryEntry::Error(
                "Invalid session id not-a-session-id".to_owned()
            ))
        );
    }

    #[test]
    fn handle_command_resume_with_extra_args_reports_error() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        assert_eq!(
            tui.handle_command(
                Command::Resume,
                vec![Uuid::from_u128(42).to_string(), "extra".to_owned()]
            ),
            None
        );

        assert_eq!(
            tui.state.history.last(),
            Some(&HistoryEntry::Error(
                "Expected at most one session id after /resume".to_owned()
            ))
        );
    }

    #[test]
    fn enter_on_resume_command_with_session_id_resumes_session() {
        let session_id = Uuid::from_u128(43);
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste(format!("/resume {session_id}")));
        let cmd = tui
            .on_input(InputEvent::Enter)
            .expect("resume command with id produces command");

        assert_eq!(cmd, Cmd::ResumeSession(session_id));
    }

    #[test]
    fn handle_command_quit_exits_application() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit.clone()));

        assert_eq!(tui.handle_command(Command::Quit, Vec::new()), None);

        assert!(exit.is_cancelled());
    }

    #[test]
    fn handle_command_unresolved_keeps_prompt_and_reports_error() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        assert_eq!(
            tui.handle_command(Command::Unresolved("wat".to_owned()), Vec::new()),
            None
        );

        assert_eq!(
            tui.state.history.last(),
            Some(&HistoryEntry::Error("Unknown command wat".to_owned()))
        );
    }

    #[test]
    fn usage_escape_restores_console_and_interrupt_exits_application() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit.clone()));
        tui.state.show_usage(usage_state());

        tui.on_input(InputEvent::Escape);

        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));

        tui.state.show_usage(usage_state());
        tui.on_input(InputEvent::Interrupt);

        assert!(exit.is_cancelled());
    }

    #[test]
    fn enter_on_models_list_sets_preferred_model_and_restores_console() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state
            .show_models_list(vec![model("first"), model(MODEL_ID)]);
        tui.on_input(InputEvent::Down);

        tui.on_input(InputEvent::Enter);

        assert_eq!(
            tui.state
                .preferred_model()
                .map(std::string::ToString::to_string)
                .as_deref(),
            Some("openai/gpt-4.1")
        );
        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));
    }

    #[test]
    fn enter_on_sessions_list_resumes_selected_session_and_restores_console() {
        let first_id = Uuid::from_u128(1);
        let second_id = Uuid::from_u128(2);
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_sessions_list(vec![
            session(first_id, "first"),
            session(second_id, "second"),
        ]);
        tui.on_input(InputEvent::Down);

        let cmd = tui
            .on_input(InputEvent::Enter)
            .expect("session selection produces a command");

        assert_eq!(cmd, Cmd::ResumeSession(second_id));
        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));
    }

    #[test]
    fn enter_on_empty_sessions_list_restores_console_without_command() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_sessions_list(Vec::new());

        assert_eq!(tui.on_input(InputEvent::Enter), None);
        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));
    }

    #[test]
    fn enter_on_invalid_model_restores_console_without_setting_preference() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_models_list(vec![Model {
            provider: "invalid provider".to_owned(),
            ..model(MODEL_ID)
        }]);

        tui.on_input(InputEvent::Enter);

        assert_eq!(tui.state.preferred_model(), None);
        assert!(matches!(
            tui.state.active_component,
            ActiveComponentState::Console(_)
        ));
    }

    fn selected_provider_index(tui: &Tui<TestBackend>) -> usize {
        tui.state
            .active_component
            .providers_list()
            .expect("providers list is active")
            .current_index()
    }

    fn selected_approval_index(tui: &Tui<TestBackend>) -> usize {
        tui.state
            .active_component
            .console()
            .expect("console view is active")
            .approval_option_index(approval_option_count(&approval_prompt()))
    }

    fn approval_cmd_for_input(event: InputEvent, prompt: ApprovalPrompt) -> Option<Cmd> {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.execution_turn = Some(ExecutionTurn::Approval(prompt));

        tui.on_input(event)
    }

    fn assert_approval_decision(cmd: Cmd, outcome: ApprovalOutcome, scope: ApprovalScope) {
        let Cmd::Continue(ContinueExecution::ApprovalDecisions { decisions }) = cmd else {
            panic!("approval decision command expected");
        };
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, APPROVAL_ID);
        assert_eq!(decisions[0].outcome, outcome);
        assert_eq!(decisions[0].scope, scope);
    }
}
