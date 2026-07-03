//! Main smista.ai cli application

use std::path::PathBuf;
use std::sync::Arc;

use smista_sdk::client::ReqwestClient;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::credentials::{ApiKeyStorage, E2eeKeysCredentials, ProvidersCredentials};
use crate::skills::SkillStore;

#[expect(dead_code, reason = "context will be used in the next tasks")]
pub struct AppContext {
    pub api_key: Arc<ApiKeyStorage>,
    pub config: Arc<Config>,
    pub cwd: PathBuf,
    pub e2ee_keys: Arc<E2eeKeysCredentials>,
    pub exit: CancellationToken,
    pub providers_credentials: Arc<ProvidersCredentials>,
    pub router_client: Arc<ReqwestClient>,
    pub skills_store: Arc<SkillStore>,
}
