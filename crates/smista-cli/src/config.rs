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
