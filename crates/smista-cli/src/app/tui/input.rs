use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::Tui;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::Cmd;
use crate::app::router_client::cmd::{
    ApprovalDecision, ApprovalOutcome, ApprovalScope, ContinueExecution,
};
use crate::app::router_client::msg::ApprovalPrompt;
use crate::app::tui::state::{
    ActiveComponentKind, ActiveComponentState, Command, ExecutionTurn, HistoryEntry,
    ModelListEntry, PromptHistoryNavigation, PromptState,
};

const EDIT_FILE_TOOL: &str = "edit_file";
const MODEL_AUTO: &str = "auto";
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
        if self.handle_file_autocomplete_input(&event) {
            return None;
        }

        if matches!(event, InputEvent::Up | InputEvent::Down) {
            return self.handle_console_history_navigation(event);
        }

        if matches!(
            &event,
            InputEvent::Backspace
                | InputEvent::Newline
                | InputEvent::Tab
                | InputEvent::Paste(_)
                | InputEvent::Delete
                | InputEvent::Char(_)
        ) {
            self.state.reset_prompt_history_navigation();
        }

        let Some(console) = self.state.active_component.console_mut() else {
            tracing::warn!("active component is not console, cannot handle input event");
            return None;
        };

        match event {
            InputEvent::Backspace => {
                console.prompt.backspace();
                self.refresh_file_autocomplete();
                None
            }
            InputEvent::Newline => {
                console.prompt.push('\n');
                self.refresh_file_autocomplete();
                None
            }
            InputEvent::Enter => match &console.prompt {
                PromptState::Empty => {
                    tracing::debug!("input event is enter, but prompt input is empty, ignoring");
                    None
                }
                PromptState::FileAutocomplete(_)
                    if console.prompt.is_command_file_autocomplete_active() =>
                {
                    console.prompt.cancel_file_autocomplete();
                    let PromptState::Command(command) = &console.prompt else {
                        unreachable!("command file completion must restore command state");
                    };
                    let prompt = console.prompt.input();
                    let (command, args) = command.resolved();
                    console.prompt.clear();
                    self.state.push_prompt_history(prompt);
                    self.handle_command(command, args)
                }
                PromptState::Text(_) | PromptState::FileAutocomplete(_) => {
                    let prompt = console.prompt.text().unwrap_or_default().trim().to_owned();
                    console.prompt.clear();
                    self.handle_prompt_exec(prompt)
                }
                PromptState::Command(command) => {
                    let prompt = console.prompt.input();
                    let (command, args) = command.resolved();
                    if !matches!(command, Command::Unresolved(_)) {
                        // clear console only if command is resolved
                        console.prompt.clear();
                        self.state.push_prompt_history(prompt);
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
                self.refresh_file_autocomplete();

                None
            }
            InputEvent::Delete => {
                console.prompt.delete();
                self.refresh_file_autocomplete();

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
                self.refresh_file_autocomplete();

                None
            }
            _ => {
                tracing::debug!("input event is not handled by the TUI scaffold");

                None
            }
        }
    }

    fn handle_prompt_exec(&mut self, prompt: String) -> Option<Cmd> {
        let files = mentioned_files(&prompt, &self.context.cwd);

        if prompt.is_empty() {
            tracing::debug!("input event is enter, but prompt input is empty, ignoring");
            None
        } else {
            tracing::debug!(
                "input event is enter, pushing prompt to history and producing execute command"
            );
            self.state
                .push_history(HistoryEntry::UserMessage(prompt.clone()));
            self.state.push_prompt_history(prompt.clone());
            Some(Cmd::Execute {
                prompt,
                files,
                plan: self.state.plan,
                explicit_model: self.state.preferred_model().cloned(),
            })
        }
    }

    fn handle_file_autocomplete_input(&mut self, event: &InputEvent) -> bool {
        let is_active = self
            .state
            .active_component
            .console()
            .is_some_and(|console| console.prompt.is_file_autocomplete_active());
        if !is_active {
            return false;
        }

        let Some(console) = self.state.active_component.console_mut() else {
            return false;
        };
        match event {
            InputEvent::Down => console.prompt.next_file_match(),
            InputEvent::Up => console.prompt.previous_file_match(),
            InputEvent::Tab | InputEvent::Right => {
                console.prompt.accept_suggestion();
                self.refresh_file_autocomplete();
            }
            InputEvent::Escape => {
                console.prompt.cancel_file_autocomplete();
            }
            _ => return false,
        }

        true
    }

    fn refresh_file_autocomplete(&mut self) {
        let query = self
            .state
            .active_component
            .console()
            .and_then(|console| console.prompt.file_autocomplete_query())
            .map(ToOwned::to_owned);
        let Some(query) = query else {
            return;
        };
        let matches = file_matches(&self.context.cwd, &query);
        if let Some(console) = self.state.active_component.console_mut() {
            console.prompt.replace_file_matches(matches);
        }
    }

    fn handle_console_history_navigation(&mut self, event: InputEvent) -> Option<Cmd> {
        let should_navigate_history = self
            .state
            .active_component
            .console()
            .map(|console| {
                console.prompt.is_empty() || self.state.is_prompt_history_navigation_active()
            })
            .unwrap_or_default();

        match (event, should_navigate_history) {
            (InputEvent::Up, true) => {
                if let Some(entry) = self.state.previous_prompt_history_entry()
                    && let Some(console) = self.state.active_component.console_mut()
                {
                    console.prompt.replace_with_input(entry);
                }
            }
            (InputEvent::Down, true) => {
                let navigation = self.state.next_prompt_history_entry();
                if let Some(console) = self.state.active_component.console_mut() {
                    match navigation {
                        PromptHistoryNavigation::Entry(entry) => {
                            console.prompt.replace_with_input(entry);
                        }
                        PromptHistoryNavigation::Clear => console.prompt.clear(),
                        PromptHistoryNavigation::Unchanged => console.prompt.move_down(),
                    }
                }
            }
            (InputEvent::Up, false) => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_up();
                }
            }
            (InputEvent::Down, false) => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_down();
                }
            }
            _ => {}
        }

        None
    }

    fn handle_command(&mut self, command: Command, args: Vec<String>) -> Option<Cmd> {
        match command {
            Command::Chat => {
                tracing::debug!("input event is chat command, exiting plan mode");
                self.state.plan = false;
                None
            }
            Command::Clear => {
                tracing::debug!("input event is clear command, producing `Clear` command");
                Some(Cmd::Clear)
            }
            Command::Model => self.handle_model_command(&args),
            Command::Plan => {
                tracing::debug!("input event is plan command, entering plan mode");
                self.state.plan = true;
                None
            }
            Command::Preview => self.handle_preview_command(&args),
            Command::Providers => {
                tracing::debug!(
                    "input event is providers command, producing list providers command"
                );
                Some(Cmd::ListProviders)
            }
            Command::Quit => {
                tracing::debug!("input event is quit command, producing exit command");
                self.context.exit.cancel();

                None
            }
            Command::Resume => self.handle_resume_command(&args),
            Command::Skills => {
                tracing::debug!("input event is skills command, producing list skills command");
                self.state.show_skill_list(
                    self.context
                        .skills_store
                        .skills()
                        .map(|(name, entry)| (name.to_owned(), entry.clone()))
                        .collect(),
                );
                None
            }
            Command::Status => {
                tracing::debug!(
                    "input event is status command, producing get router status command"
                );
                Some(Cmd::GetRouterStatus)
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

    fn handle_model_command(&mut self, args: &[String]) -> Option<Cmd> {
        match args {
            [] => {
                tracing::debug!("input event is model command, listing models");

                Some(Cmd::ListModels)
            }
            model => {
                let model = model.join(" ");
                if model == MODEL_AUTO {
                    tracing::debug!("input event is model auto command, clearing preferred model");
                    self.state.clear_preferred_model();

                    return None;
                }

                tracing::debug!(%model, "input event is model command with model id; awaiting list of models to validate");
                self.state.set_awaited_model(model);

                Some(Cmd::ListModels)
            }
        }
    }

    fn handle_preview_command(&mut self, args: &[String]) -> Option<Cmd> {
        tracing::debug!("input event is preview command, producing preview routing command");
        if args.is_empty() {
            tracing::debug!("input event is preview command, but no prompt was provided");
            self.state.push_history(HistoryEntry::Error(
                "No prompt provided for preview command".to_owned(),
            ));
            return None;
        }

        let prompt = args.join(" ");
        let files = mentioned_files(&prompt, &self.context.cwd);
        Some(Cmd::Preview {
            prompt,
            files,
            plan: self.state.plan,
            explicit_model: self.state.preferred_model().cloned(),
        })
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

    /// Handles an interrupt input event (e.g., Ctrl+C) and produces a command to break execution or clear the console prompt.
    ///
    /// 1. If there is an active execution turn or the router is thinking,
    ///    it produces a `Cmd::Continue(ContinueExecution::Break)` command to break the execution.
    /// 2. If the console prompt is not empty, it clears the prompt input.
    /// 3. If neither of the above conditions is met, it cancels the application exit.
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
        } else if let Some(console) = self.state.active_component.console_mut()
            && !console.prompt.is_empty()
        {
            tracing::debug!("input event is interrupt, clearing console prompt input");
            console.prompt.clear();
            None
        } else {
            tracing::debug!("input event is interrupt, but router is not thinking, exiting");
            self.context.exit.cancel();
            None
        }
    }

    fn set_selected_model_as_preferred(&mut self) {
        let Some(entry) = self
            .state
            .active_component
            .models_list()
            .and_then(|list| list.selected())
        else {
            return;
        };

        match entry {
            ModelListEntry::Auto => self.state.clear_preferred_model(),
            ModelListEntry::Model(model) => self.state.set_preferred_model(model.reference.clone()),
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

fn file_matches(cwd: &Path, query: &str) -> Vec<PathBuf> {
    let (typed_parent, prefix) = query
        .rfind(std::path::is_separator)
        .map_or(("", query), |separator| query.split_at(separator + 1));
    let parent = Path::new(typed_parent);
    let resolved_parent = if parent.is_absolute() {
        parent.to_owned()
    } else {
        cwd.join(parent)
    };
    let Ok(entries) = std::fs::read_dir(resolved_parent) else {
        return Vec::new();
    };

    let mut matches = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| {
            let mut path = PathBuf::from(typed_parent).join(entry.file_name());
            if entry.path().is_dir() {
                path.push("");
            }
            path
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

fn mentioned_files(prompt: &str, cwd: &Path) -> HashSet<PathBuf> {
    prompt
        .split_whitespace()
        .filter_map(|token| token.strip_prefix('@'))
        .filter(|path| !path.is_empty())
        .map(Path::new)
        .map(|path| {
            if path.is_absolute() {
                path.to_owned()
            } else {
                cwd.join(path)
            }
        })
        .filter(|path| path.is_file() && std::fs::File::open(path).is_ok())
        .collect()
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
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Instant;

    use ratatui::backend::TestBackend;
    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use smista_sdk::core::api::SessionUsageResponse;
    use smista_sdk::core::model::ModelReference;
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
    const SKILL_DESCRIPTION: &str = "List available skills.";
    const SKILL_NAME: &str = "list-skills";

    fn app_context(exit: CancellationToken) -> AppContext {
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        app_context_for_cwd(exit, cwd)
    }

    fn app_context_for_cwd(exit: CancellationToken, cwd: PathBuf) -> AppContext {
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

    fn write_project_skill(cwd: &Path, name: &str, description: &str) {
        let skill_dir = cwd.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).expect("skill directory is created");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nUse this skill.\n"),
        )
        .expect("skill descriptor is written");
    }

    fn model(id: &str) -> Model {
        Model {
            reference: ModelReference {
                provider: smista_sdk::core::model::Provider::OpenAI,
                model: id.to_owned(),
            },
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
    fn file_matcher_lists_sorted_immediate_entries() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::write(cwd.path().join("zeta"), "z").expect("file is written");
        std::fs::write(cwd.path().join("alpha"), "a").expect("file is written");
        std::fs::write(cwd.path().join(".hidden"), "h").expect("file is written");
        std::fs::create_dir(cwd.path().join("directory")).expect("directory is created");

        let matches = file_matches(cwd.path(), "");
        let mut directory = PathBuf::from("directory");
        directory.push("");

        assert_eq!(
            matches,
            vec![
                PathBuf::from(".hidden"),
                PathBuf::from("alpha"),
                directory,
                PathBuf::from("zeta"),
            ]
        );
    }

    #[test]
    fn file_matcher_uses_case_sensitive_prefixes_and_includes_ignored_entries() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::write(cwd.path().join("Alpha"), "a").expect("file is written");
        std::fs::write(cwd.path().join("alpha"), "a").expect("file is written");
        std::fs::write(cwd.path().join("ignored.log"), "i").expect("file is written");
        std::fs::write(cwd.path().join(".gitignore"), "ignored.log\n")
            .expect("gitignore is written");

        assert_eq!(file_matches(cwd.path(), "Al"), vec![PathBuf::from("Alpha")]);
        assert_eq!(
            file_matches(cwd.path(), "ignored"),
            vec![PathBuf::from("ignored.log")]
        );
    }

    #[test]
    fn file_matcher_resolves_nested_relative_and_absolute_parents() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        let nested = cwd.path().join("src");
        std::fs::create_dir(&nested).expect("directory is created");
        std::fs::write(nested.join("lib.rs"), "lib").expect("file is written");

        let relative_query = format!("src{}li", std::path::MAIN_SEPARATOR);
        assert_eq!(
            file_matches(cwd.path(), &relative_query),
            vec![PathBuf::from("src").join("lib.rs")]
        );

        let absolute_query = format!("{}{}li", nested.display(), std::path::MAIN_SEPARATOR);
        assert_eq!(
            file_matches(cwd.path(), &absolute_query),
            vec![nested.join("lib.rs")]
        );
    }

    #[test]
    fn file_matcher_handles_special_names_and_missing_parents() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::write(cwd.path().join("with@sign"), "a").expect("file is written");
        std::fs::write(cwd.path().join("unicodé"), "u").expect("file is written");

        assert_eq!(
            file_matches(cwd.path(), "with@"),
            vec![PathBuf::from("with@sign")]
        );
        assert_eq!(
            file_matches(cwd.path(), "unic"),
            vec![PathBuf::from("unicodé")]
        );
        assert!(file_matches(cwd.path(), "missing/child").is_empty());
        assert!(file_matches(cwd.path(), "no-match").is_empty());
    }

    #[test]
    fn file_matcher_preserves_dot_and_parent_components() {
        let root = tempfile::tempdir().expect("temporary directory is created");
        let cwd = root.path().join("child");
        std::fs::create_dir(&cwd).expect("child directory is created");
        std::fs::write(cwd.join("local"), "local").expect("file is written");
        std::fs::write(root.path().join("parent"), "parent").expect("file is written");

        assert_eq!(
            file_matches(&cwd, &format!(".{}lo", std::path::MAIN_SEPARATOR)),
            vec![PathBuf::from(".").join("local")]
        );
        assert_eq!(
            file_matches(&cwd, &format!("..{}pa", std::path::MAIN_SEPARATOR)),
            vec![PathBuf::from("..").join("parent")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_matcher_includes_symlinks_and_tolerates_unreadable_parents() {
        use std::os::unix::fs::PermissionsExt;

        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::write(cwd.path().join("target"), "target").expect("file is written");
        std::os::unix::fs::symlink(cwd.path().join("target"), cwd.path().join("link"))
            .expect("symlink is created");
        assert_eq!(file_matches(cwd.path(), "li"), vec![PathBuf::from("link")]);

        let unreadable = cwd.path().join("unreadable");
        std::fs::create_dir(&unreadable).expect("directory is created");
        std::fs::write(unreadable.join("child"), "child").expect("file is written");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o0))
            .expect("directory permissions are changed");
        if std::fs::read_dir(&unreadable).is_err() {
            assert!(
                file_matches(
                    cwd.path(),
                    &format!("unreadable{}", std::path::MAIN_SEPARATOR)
                )
                .is_empty()
            );
        }
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o700))
            .expect("directory permissions are restored");
    }

    #[cfg(windows)]
    #[test]
    fn file_matcher_supports_windows_drive_and_unc_paths() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::write(cwd.path().join("rooted"), "rooted").expect("file is written");
        let drive_query = format!("{}{}roo", cwd.path().display(), std::path::MAIN_SEPARATOR);

        assert_eq!(
            file_matches(cwd.path(), &drive_query),
            vec![cwd.path().join("rooted")]
        );
        assert!(file_matches(cwd.path(), r"\\missing\share\file").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn file_matcher_supports_unix_rooted_paths() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        let name = cwd
            .path()
            .file_name()
            .expect("temporary directory has a name")
            .to_string_lossy();
        let parent = cwd
            .path()
            .parent()
            .expect("temporary directory has a parent");
        let query = format!(
            "{}/{}",
            parent.display(),
            &name[..name.len().saturating_sub(1)]
        );

        assert!(file_matches(cwd.path(), &query).contains(&cwd.path().to_path_buf()));
    }

    #[test]
    fn typing_and_editing_file_mention_refreshes_matches() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        std::fs::write(cwd.join("alpha"), "a").expect("file is written");
        std::fs::write(cwd.join("beta"), "b").expect("file is written");
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Char('@'));
        tui.on_input(InputEvent::Char('b'));
        assert_eq!(console_suggestion(&tui), Some("@beta"));

        tui.on_input(InputEvent::Backspace);
        assert!(console_suggestion(&tui).is_some());
    }

    #[test]
    fn file_completion_navigation_acceptance_and_escape_precede_history() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        std::fs::write(cwd.join("alpha"), "a").expect("file is written");
        std::fs::write(cwd.join("beta"), "b").expect("file is written");
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Char('@'));
        tui.on_input(InputEvent::Down);
        assert_eq!(console_suggestion(&tui), Some("@beta"));
        tui.on_input(InputEvent::Up);
        assert_eq!(console_suggestion(&tui), Some("@alpha"));
        tui.on_input(InputEvent::Tab);
        assert_eq!(console_prompt_input(&tui), "@alpha");
        tui.on_input(InputEvent::Escape);
        assert_eq!(console_prompt_input(&tui), "@alpha");
        assert!(!console_file_autocomplete_active(&tui));
    }

    #[test]
    fn right_accepts_file_match_and_is_safe_without_matches() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        std::fs::write(cwd.join("file.rs"), "f").expect("file is written");
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Paste("@fi".to_owned()));
        tui.on_input(InputEvent::Right);
        assert_eq!(console_prompt_input(&tui), "@file.rs");

        tui.on_input(InputEvent::Char('x'));
        assert_eq!(console_prompt_input(&tui), "@file.rsx");
        tui.on_input(InputEvent::Right);
        assert_eq!(console_prompt_input(&tui), "@file.rsx");
    }

    #[test]
    fn accepted_directory_can_complete_nested_file() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        std::fs::create_dir(cwd.join("src")).expect("directory is created");
        std::fs::write(cwd.join("src").join("lib.rs"), "lib").expect("file is written");
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Paste("@sr".to_owned()));
        tui.on_input(InputEvent::Tab);
        assert_eq!(
            console_prompt_input(&tui),
            format!("@src{}", std::path::MAIN_SEPARATOR)
        );
        assert!(console_suggestion(&tui).is_some_and(|path| path.ends_with("lib.rs")));
        tui.on_input(InputEvent::Tab);
        assert!(console_prompt_input(&tui).ends_with("lib.rs"));
    }

    #[test]
    fn paste_whitespace_ends_file_completion() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("review @file next".to_owned()));

        assert!(!console_file_autocomplete_active(&tui));
        assert_eq!(console_prompt_input(&tui), "review @file next");
    }

    #[test]
    fn preview_completes_and_collects_file_mentions_without_at_prefix() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        let file = cwd.join("file.rs");
        std::fs::write(&file, "file").expect("file is written");
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Paste("/preview review @fi".to_owned()));
        assert_eq!(console_suggestion(&tui), Some("/preview review @file.rs"));
        tui.on_input(InputEvent::Tab);

        let cmd = tui
            .on_input(InputEvent::Enter)
            .expect("preview command is produced");
        let Cmd::Preview { prompt, files, .. } = cmd else {
            panic!("preview command expected");
        };

        assert_eq!(prompt, "review @file.rs");
        assert_eq!(files, HashSet::from([file]));
        assert!(
            files
                .iter()
                .all(|path| !path.to_string_lossy().contains('@'))
        );
    }

    #[test]
    fn second_escape_after_file_completion_cancellation_clears_prompt() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.on_input(InputEvent::Paste("@missing".to_owned()));

        tui.on_input(InputEvent::Escape);
        assert_eq!(console_prompt_input(&tui), "@missing");

        tui.on_input(InputEvent::Escape);
        assert_eq!(console_prompt_input(&tui), "");
    }

    #[test]
    fn enter_resolves_file_mentions_and_preserves_prompt_text() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        let relative = cwd.join("relative.rs");
        std::fs::write(&relative, "relative").expect("file is written");
        let outside = tempfile::NamedTempFile::new().expect("temporary file is created");
        let prompt = format!(
            "  review @relative.rs @relative.rs @{} @missing @  ",
            outside.path().display()
        );
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Paste(prompt));
        let cmd = tui
            .on_input(InputEvent::Enter)
            .expect("text prompt produces execute command");
        let Cmd::Execute {
            prompt,
            files,
            plan,
            explicit_model,
        } = cmd
        else {
            panic!("execute command expected");
        };

        assert_eq!(
            prompt,
            format!(
                "review @relative.rs @relative.rs @{} @missing @",
                outside.path().display()
            )
        );
        assert_eq!(
            files,
            HashSet::from([relative, outside.path().to_path_buf()])
        );
        assert!(!plan);
        assert_eq!(explicit_model, None);

        tui.on_input(InputEvent::Up);
        assert_eq!(console_prompt_input(&tui), prompt);
    }

    #[test]
    fn mentioned_files_omit_directories_and_keep_symlink_targets() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        std::fs::create_dir(cwd.path().join("directory")).expect("directory is created");
        std::fs::write(cwd.path().join("target"), "target").expect("file is written");

        #[cfg(unix)]
        std::os::unix::fs::symlink(cwd.path().join("target"), cwd.path().join("link"))
            .expect("symlink is created");

        let files = mentioned_files("@directory @target @missing @link", cwd.path());
        assert!(files.contains(&cwd.path().join("target")));
        assert!(!files.contains(&cwd.path().join("directory")));
        assert!(!files.contains(&cwd.path().join("missing")));
        #[cfg(unix)]
        assert!(files.contains(&cwd.path().join("link")));
    }

    #[test]
    fn mentioned_files_parse_all_whitespace_delimiters() {
        let cwd = tempfile::tempdir().expect("temporary directory is created");
        for name in ["one", "two", "three"] {
            std::fs::write(cwd.path().join(name), name).expect("file is written");
        }

        let files = mentioned_files("@one\t@two\n@three", cwd.path());

        assert_eq!(
            files,
            HashSet::from([
                cwd.path().join("one"),
                cwd.path().join("two"),
                cwd.path().join("three"),
            ])
        );
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
    fn up_on_empty_console_recalls_latest_prompt_history_entry() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("first prompt".to_owned()));
        tui.on_input(InputEvent::Enter);
        tui.on_input(InputEvent::Paste("second prompt".to_owned()));
        tui.on_input(InputEvent::Enter);

        tui.on_input(InputEvent::Up);

        assert_eq!(console_prompt_input(&tui), "second prompt");
    }

    #[test]
    fn up_and_down_navigate_prompt_history_entries() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("first prompt".to_owned()));
        tui.on_input(InputEvent::Enter);
        tui.on_input(InputEvent::Paste("second prompt".to_owned()));
        tui.on_input(InputEvent::Enter);

        tui.on_input(InputEvent::Up);
        tui.on_input(InputEvent::Up);
        assert_eq!(console_prompt_input(&tui), "first prompt");

        tui.on_input(InputEvent::Down);
        assert_eq!(console_prompt_input(&tui), "second prompt");

        tui.on_input(InputEvent::Down);
        assert_eq!(console_prompt_input(&tui), "");
    }

    #[test]
    fn enter_on_command_saves_command_to_prompt_history() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("/providers".to_owned()));
        tui.on_input(InputEvent::Enter);
        tui.state.show_console();
        tui.on_input(InputEvent::Up);

        assert_eq!(console_prompt_input(&tui), "/providers");
    }

    #[test]
    fn unresolved_command_is_not_saved_to_prompt_history() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("/unknown".to_owned()));
        tui.on_input(InputEvent::Enter);
        tui.on_input(InputEvent::Interrupt);
        tui.on_input(InputEvent::Up);

        assert_eq!(console_prompt_input(&tui), "");
    }

    #[test]
    fn up_on_non_empty_console_keeps_prompt_cursor_navigation() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        tui.on_input(InputEvent::Paste("history prompt".to_owned()));
        tui.on_input(InputEvent::Enter);
        tui.on_input(InputEvent::Paste("draft\nprompt".to_owned()));
        tui.on_input(InputEvent::Up);

        assert_eq!(console_prompt_input(&tui), "draft\nprompt");
        assert_eq!(
            tui.state
                .active_component
                .console()
                .expect("console view is active")
                .prompt
                .cursor_position(),
            "draft".chars().count()
        );
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
    fn interrupt_clears_console_prompt_without_exiting() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit.clone()));
        tui.on_input(InputEvent::Paste("draft prompt".to_owned()));

        assert_eq!(tui.on_input(InputEvent::Interrupt), None);

        let console = tui
            .state
            .active_component
            .console()
            .expect("console view is active");
        assert_eq!(console.prompt.input(), "");
        assert!(!exit.is_cancelled());
    }

    #[test]
    fn interrupt_on_empty_console_exits_application() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit.clone()));

        assert_eq!(tui.on_input(InputEvent::Interrupt), None);

        assert!(exit.is_cancelled());
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
    fn handle_command_resume_plan() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        assert!(!tui.state.plan);
        assert!(tui.handle_command(Command::Plan, Vec::new()).is_none());
        assert!(tui.state.plan);
    }

    #[test]
    fn handle_command_resume_chat() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.plan = true;

        assert!(tui.state.plan);
        assert!(tui.handle_command(Command::Chat, Vec::new()).is_none());
        assert!(!tui.state.plan);
    }

    #[test]
    fn handle_command_clear_requests_session_clear() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Clear, Vec::new())
            .expect("clear command produces command");

        assert_eq!(cmd, Cmd::Clear);
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
    fn handle_command_model_without_args_lists_models() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Model, Vec::new())
            .expect("model command produces command");

        assert_eq!(cmd, Cmd::ListModels);
        assert_eq!(tui.state.take_awaited_model(), None);
    }

    #[test]
    fn handle_command_model_with_name_lists_models_for_validation() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Model, vec![MODEL_ID.to_owned()])
            .expect("model command with name produces command");

        assert_eq!(cmd, Cmd::ListModels);
        assert_eq!(tui.state.take_awaited_model().as_deref(), Some(MODEL_ID));
    }

    #[test]
    fn handle_command_model_auto_clears_preferred_model() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.set_preferred_model(
            format!("{PROVIDER_OPENAI}/{MODEL_ID}")
                .parse::<ModelReference>()
                .expect("model reference parses"),
        );

        assert_eq!(
            tui.handle_command(Command::Model, vec!["auto".to_owned()]),
            None
        );

        assert_eq!(tui.state.preferred_model(), None);
        assert_eq!(tui.state.take_awaited_model(), None);
    }

    #[test]
    fn handle_command_providers_lists_providers() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

        let cmd = tui
            .handle_command(Command::Providers, Vec::new())
            .expect("providers command produces command");

        assert_eq!(cmd, Cmd::ListProviders);
    }

    #[test]
    fn handle_command_skills_shows_discovered_skill_names() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        write_project_skill(&cwd, SKILL_NAME, SKILL_DESCRIPTION);
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        assert_eq!(tui.handle_command(Command::Skills, Vec::new()), None);

        let skills = tui
            .state
            .active_component
            .skill_list()
            .expect("skill list is active");
        let (_name, entry) = skills
            .entries()
            .iter()
            .find(|(name, _entry)| name == SKILL_NAME)
            .expect("project skill is listed");
        assert_eq!(entry.description(), SKILL_DESCRIPTION);
        assert!(tui.state.history.is_empty());
    }

    #[test]
    fn enter_on_skills_command_shows_discovered_skill_names() {
        let exit = CancellationToken::new();
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        write_project_skill(&cwd, SKILL_NAME, SKILL_DESCRIPTION);
        let mut tui = Tui::<TestBackend>::new_test(app_context_for_cwd(exit, cwd));

        tui.on_input(InputEvent::Paste("/skills".to_owned()));
        assert_eq!(tui.on_input(InputEvent::Enter), None);

        let skills = tui
            .state
            .active_component
            .skill_list()
            .expect("skill list is active");
        let (_name, entry) = skills
            .entries()
            .iter()
            .find(|(name, _entry)| name == SKILL_NAME)
            .expect("project skill is listed");
        assert_eq!(entry.description(), SKILL_DESCRIPTION);
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
    fn enter_on_models_list_auto_clears_preferred_model_and_restores_console() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.set_preferred_model(
            format!("{PROVIDER_OPENAI}/{MODEL_ID}")
                .parse::<ModelReference>()
                .expect("model reference parses"),
        );
        tui.state.show_models_list(vec![model(MODEL_ID)]);

        tui.on_input(InputEvent::Enter);

        assert_eq!(tui.state.preferred_model(), None);
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
    fn enter_on_model_uses_reference_and_restores_console() {
        let exit = CancellationToken::new();
        let mut tui = Tui::<TestBackend>::new_test(app_context(exit));
        tui.state.show_models_list(vec![Model {
            provider: "invalid provider".to_owned(),
            ..model(MODEL_ID)
        }]);
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

    fn selected_provider_index(tui: &Tui<TestBackend>) -> usize {
        tui.state
            .active_component
            .providers_list()
            .expect("providers list is active")
            .current_index()
    }

    fn console_prompt_input(tui: &Tui<TestBackend>) -> String {
        tui.state
            .active_component
            .console()
            .expect("console view is active")
            .prompt
            .input()
    }

    fn console_suggestion(tui: &Tui<TestBackend>) -> Option<&str> {
        tui.state
            .active_component
            .console()
            .expect("console view is active")
            .prompt
            .current_suggestion()
    }

    fn console_file_autocomplete_active(tui: &Tui<TestBackend>) -> bool {
        tui.state
            .active_component
            .console()
            .expect("console view is active")
            .prompt
            .is_file_autocomplete_active()
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
