use std::sync::Arc;

use smista_sdk::client::{ReqwestClient, RouterClientConfig};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::*;
use crate::app::router_client::msg::AssistantTurn;
use crate::app::tui::state::HistoryEntry;
use crate::config::Config;
use crate::credentials::{
    ApiKeyStorage, CredentialBackend, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
};
use crate::skills::SkillStore;

const ASSISTANT_MESSAGE: &str = "hello";

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
}
