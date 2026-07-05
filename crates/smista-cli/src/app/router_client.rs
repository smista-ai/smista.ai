mod protocol;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

pub use self::protocol::{Cmd, Msg};
use crate::app::AppContext;

/// Worker responsible for communicating with `smista-router`.
///
/// This scaffold owns the command receiver and message sender, but it does not
/// execute router requests yet. Future tasks will translate [`Cmd`] values into
/// authenticated HTTP calls and emit [`Msg`] updates.
pub struct RouterClient {
    cmd_rx: Receiver<Cmd>,
    context: AppContext,
    #[expect(
        dead_code,
        reason = "Router messages are emitted once real router calls are implemented."
    )]
    msg_tx: Sender<Msg>,
}

impl RouterClient {
    /// Creates a router client worker from its command and message channels.
    #[must_use]
    pub fn new(cmd_rx: Receiver<Cmd>, msg_tx: Sender<Msg>, context: AppContext) -> Self {
        Self {
            cmd_rx,
            msg_tx,
            context,
        }
    }

    /// Spawns the router client worker task.
    #[must_use]
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.run_loop())
    }

    async fn run_loop(mut self) {
        tracing::debug!("RouterClient started");

        loop {
            tokio::select! {
                _ = self.context.exit.cancelled() => {
                    break;
                }
                Some(cmd) = self.cmd_rx.recv() => {
                    tracing::debug!("RouterClient received command: {cmd:?}");
                    tracing::trace!("RouterClient scaffold ignored command until execution is implemented");
                }
            }
        }

        tracing::info!("RouterClient stopped");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use tokio_util::sync::CancellationToken;
    use url::Url;

    use super::*;
    use crate::config::Config;
    use crate::credentials::{
        ApiKeyStorage, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
    };
    use crate::skills::SkillStore;

    #[tokio::test]
    async fn should_ignore_scaffold_commands_until_cancelled() {
        let exit = CancellationToken::new();
        let context = app_context(exit.clone());
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
        let (msg_tx, _msg_rx) = tokio::sync::mpsc::channel(1);
        let worker = RouterClient::new(cmd_rx, msg_tx, context).run();

        cmd_tx
            .send(Cmd::Execute {
                prompt: "hello".to_owned(),
                files: HashMap::default(),
            })
            .await
            .expect("router worker receives scaffold commands");
        tokio::time::sleep(Duration::from_millis(10)).await;

        exit.cancel();
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .expect("router worker stops after cancellation")
            .expect("router worker does not panic on scaffold commands");
    }

    fn app_context(exit: CancellationToken) -> AppContext {
        let cwd = tempfile::tempdir()
            .expect("temporary directory is created")
            .keep();
        let credentials =
            Arc::new(CredentialsStorage::new(false).expect("test credentials storage builds"));
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
}
