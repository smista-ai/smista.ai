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

// The configuration subsystem is fully built but, apart from path resolution,
// is not yet wired into a command: the interactive CLI consumes these types in
// milestone M6. `expect` (rather than `allow`) makes the lint fire again — and
// forces this annotation to be removed — the moment they are actually used.
#![expect(
    dead_code,
    unused_imports,
    reason = "config types are consumed by the interactive CLI in milestone M6"
)]

pub mod layers;
pub mod paths;
pub mod secrets;
pub mod skills;
pub mod validate;

mod load;
mod model;

pub use layers::{ConfigLayer, merge};
pub use load::{ConfigError, load, parse};
pub use model::{AuthSource, Config, LocalPreferences, ProviderConfig, RouterClientConfig};
pub use secrets::{SecretError, SecretResolver};
pub use skills::{SkillEntry, SkillError, SkillStore, SkillWarning};
#[doc(inline)]
pub use validate::{Severity, ValidationCode, ValidationError, ValidationReport};
