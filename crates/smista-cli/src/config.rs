//! CLI/policy configuration: the `config.toml` model and its layered merge.
//!
//! The CLI loads configuration from up to three on-disk layers — global,
//! project and uncommitted local preferences — and merges them with documented
//! precedence (see [`layers`]). The merge strategy is chosen per section rather
//! than being uniform: `providers`/`models` maps are replaced when the higher
//! layer is non-empty; `routing` and `classification` are replaced wholesale
//! when the higher layer defines them (each is a cohesive unit); router client
//! settings and local preferences are merged field-by-field (a higher `Some`
//! wins and `router.auto_start` latches on); and privacy path lists are unioned
//! while `local_only`/`no_network` and tool `Deny` permissions latch on and are
//! never weakened. See [`layers`] for the full per-field rules.
//!
//! The merged [`Config`] aggregates the shared policy types from `smista-core`,
//! which the CLI then sends to the router; the router evaluates those same
//! types. Path resolution lives in [`paths`] and secret resolution in
//! [`secrets`].

pub mod layers;
pub mod paths;
pub mod secrets;

mod load;
mod model;

pub use layers::{ConfigLayer, merge};
pub use load::{ConfigError, load, parse};
pub use model::{AuthSource, Config, LocalPreferences, ProviderConfig, RouterClientConfig};
pub use secrets::{SecretError, SecretResolver};
