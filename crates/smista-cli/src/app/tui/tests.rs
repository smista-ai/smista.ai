use std::sync::Arc;
use std::time::Instant;

use smista_sdk::client::{ReqwestClient, RouterClientConfig};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::*;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::msg::AssistantTurn;
use crate::app::tui::state::{HistoryEntry, RouterState};
use crate::config::Config;
use crate::credentials::{
    ApiKeyStorage, CredentialBackend, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
};
use crate::skills::SkillStore;

const ASSISTANT_MESSAGE: &str = "hello";
const PROMPT_PLACEHOLDER: &str = "Type a message or /command";

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
        api_key: Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd)),
        config: Arc::new(Config::default()),
        cwd: cwd.clone(),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit,
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
    }
}

#[test]
fn should_render_default_view() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.view().expect("TUI view renders without error");

    assert_backend_contains(&tui, PROMPT_PLACEHOLDER);
    assert!(tui.terminal.backend().cursor_visible());
    assert!(tui.terminal.backend().cursor_position().y >= 20);
}

#[test]
fn handle_client_msg_applies_message_to_state() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.handle_client_msg(Msg::AssistantTurn(AssistantTurn {
        message: ASSISTANT_MESSAGE.to_owned(),
        trace_id: None,
    }))
    .expect("client message is handled");

    assert_eq!(
        tui.state.history,
        vec![HistoryEntry::AssistantMessage(ASSISTANT_MESSAGE.to_owned())]
    );
    assert_backend_contains(&tui, ASSISTANT_MESSAGE);
    assert_backend_contains(&tui, PROMPT_PLACEHOLDER);
}

#[test]
fn handle_input_event_prints_history_entries() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.state.push_history(HistoryEntry::AssistantMessage(
        "Hello, this is an assistant message".to_owned(),
    ));
    tui.view().expect("TUI view renders without error");

    assert_backend_contains(&tui, "Hello, this is an assistant message");
    assert_backend_contains(&tui, PROMPT_PLACEHOLDER);
    assert!(tui.terminal.backend().cursor_visible());
}

#[test]
fn enter_submits_prompt_to_history() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.handle_input_event(InputEvent::Char('a'))
        .expect("input character is handled");
    tui.handle_input_event(InputEvent::Enter)
        .expect("enter is handled");

    assert_eq!(
        tui.state.history,
        vec![HistoryEntry::UserMessage("a".to_owned())]
    );
    assert_backend_contains(&tui, "a");
    assert_backend_contains(&tui, PROMPT_PLACEHOLDER);
    assert!(tui.terminal.backend().cursor_visible());
}

#[test]
fn pushed_history_keeps_latest_entry_visible() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    for index in 0..12 {
        tui.state
            .push_history(HistoryEntry::AssistantMessage(format!("message-{index}")));
        tui.view().expect("TUI view renders without error");
    }

    assert_backend_contains(&tui, "message-11");
}

#[test]
fn newline_adds_line_break_to_prompt() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.handle_input_event(InputEvent::Char('a'))
        .expect("input character is handled");
    tui.handle_input_event(InputEvent::Newline)
        .expect("newline is handled");
    tui.handle_input_event(InputEvent::Char('b'))
        .expect("input character is handled");

    let console = tui
        .state
        .active_component
        .console()
        .expect("console view is active");
    assert_eq!(console.prompt.input(), "a\nb");
    assert!(tui.state.history.is_empty());
}

#[test]
fn input_events_edit_at_prompt_cursor() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.handle_input_event(InputEvent::Char('a'))
        .expect("input character is handled");
    tui.handle_input_event(InputEvent::Char('c'))
        .expect("input character is handled");
    tui.handle_input_event(InputEvent::Left)
        .expect("left arrow is handled");
    tui.handle_input_event(InputEvent::Char('b'))
        .expect("input character is handled");
    tui.handle_input_event(InputEvent::Right)
        .expect("right arrow is handled");
    tui.handle_input_event(InputEvent::Backspace)
        .expect("backspace is handled");
    tui.handle_input_event(InputEvent::Left)
        .expect("left arrow is handled");
    tui.handle_input_event(InputEvent::Delete)
        .expect("delete is handled");

    let console = tui
        .state
        .active_component
        .console()
        .expect("console view is active");
    assert_eq!(console.prompt.input(), "a");
    assert_eq!(console.prompt.cursor_position(), 1);
}

#[test]
fn refresh_only_renders_while_thinking() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    assert!(!tui.refresh().expect("idle refresh succeeds"));

    tui.state.router = RouterState::Thinking(Instant::now());

    assert!(tui.refresh().expect("thinking refresh succeeds"));
    assert_backend_contains(&tui, "Working (0s - esc to interrupt)");
}

#[test]
fn refresh_does_not_insert_pending_history_entries() {
    let exit = CancellationToken::new();
    let mut tui = Tui::<TestBackend>::new_test(app_context(exit));

    tui.state
        .push_history(HistoryEntry::AssistantMessage("pending-history".to_owned()));
    tui.state.router = RouterState::Thinking(Instant::now());

    assert!(tui.refresh().expect("thinking refresh succeeds"));

    assert_eq!(tui.printed_history_entries, 0);
    assert!(
        !tui.terminal
            .backend()
            .to_string()
            .contains("pending-history")
    );
}

fn assert_backend_contains(tui: &Tui<TestBackend>, expected: &str) {
    let screen = tui.terminal.backend().to_string();
    assert!(
        screen.contains(expected),
        "expected terminal screen to contain {expected:?}\n{screen}",
    );
}
