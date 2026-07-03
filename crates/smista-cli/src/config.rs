//! CLI/policy configuration: the `config.toml` model and its layered merge.
//!
//! The CLI loads configuration from global and project layers and may apply a
//! runtime override. Merge precedence is documented in [`layers`]. The merge
//! strategy is chosen per section rather than being uniform: `providers` maps
//! are replaced when the higher layer is non-empty; `routing` and
//! `classification` are replaced wholesale when the higher layer defines them
//! (each is a cohesive unit); router client settings and local preferences are
//! merged field-by-field (a higher `Some` wins and `router.auto_start` latches
//! on); and privacy path lists are unioned while tool `Deny` permissions latch
//! on and are never weakened. See [`layers`] for the full per-field rules.
//!
//! The merged [`Config`] aggregates the shared policy types from `smista-core`,
//! which the CLI then sends to the router; the router evaluates those same
//! types. Config path resolution lives in [`paths`].

pub mod layers;
pub mod paths;
pub mod validate;

mod load;
mod model;

use std::path::Path;

#[cfg(test)]
pub use load::parse;

use self::load::load_with_layers;
#[doc(inline)]
pub use self::model::Config;
use crate::config::load::load_at;

pub fn load_and_validate(cwd: &Path) -> anyhow::Result<Config> {
    let (config, layers) = load_with_layers(cwd, None)?;
    let report = validate::validate_layers(&config, &layers);

    if !report.is_ok() {
        anyhow::bail!(
            "CLI configuration validation failed: {report}",
            report = report.to_human()
        );
    }
    Ok(config)
}

/// Loads the effective CLI configuration for `cwd`.
///
/// The result is the merged configuration after applying built-in defaults,
/// global configuration, and project configuration in precedence order.
///
/// # Errors
///
/// Returns an error when an existing configuration layer cannot be read or
/// parsed.
pub fn load_effective(cwd: &Path) -> anyhow::Result<Config> {
    let (config, _) = load_with_layers(cwd, None)?;
    Ok(config)
}

/// Loads and validates a single CLI configuration file.
///
/// # Errors
///
/// Returns an error when `config_path` cannot be read, cannot be parsed, or
/// fails CLI configuration validation.
pub fn load_and_validate_at(config_path: &Path) -> anyhow::Result<Config> {
    let config = load_at(config_path)?;
    let report = validate::validate_layers(&config, &[]);

    if !report.is_ok() {
        anyhow::bail!(
            "CLI configuration validation failed: {report}",
            report = report.to_human()
        );
    }
    Ok(config)
}
