//! Layered configuration merge.
//!
//! Configuration is assembled from ordered layers, folded low→high so higher
//! layers win. The merge strategy is *not* uniform; it is chosen per section to
//! protect cohesive units and safety-critical fields. Layers split into two
//! kinds: **config layers** (`SystemDefaults`, `Global`, `Project`) and
//! **preference layers** (`LocalPrefs`, `RuntimeOverride`). The safety invariant
//! is that a *preference* layer may tighten but never weaken a safety-critical
//! field set by a config layer (spec: "a local user preference must not silently
//! bypass this restriction").
//!
//! - **`providers` map**: replaced wholesale when the higher layer is non-empty,
//!   otherwise the lower map is kept.
//! - **`routing` / `classification`**: replaced wholesale when the higher layer
//!   defines a non-default value. Each is a cohesive unit, never partially
//!   merged.
//! - **`router` (client settings)**: merged field-by-field. Optional fields take
//!   the higher layer's `Some`; `auto_start` latches on.
//! - **`local` (local preferences)**: merged field-by-field; higher-layer `Some`
//!   wins.
//! - **`privacy` path lists** (`restricted_paths`, `remote.blocked_paths`):
//!   always unioned, so a higher layer can only *add* restrictions.
//! - **`privacy.remote.mode` / `privacy.local.mode`**: a config layer overrides;
//!   a preference layer may only make the mode *stricter*
//!   (`allow < ask < deny`), never weaker.
//! - **`tools.permissions`**: a config layer overrides per tool; a preference
//!   layer may only make a tool's mode stricter, so a lower-layer `deny` is never
//!   weakened.

use std::collections::BTreeMap;

use smista_sdk::core::policy::{PermissionMode, PrivacyPolicy, ToolsConfig};

use super::model::{Config, LocalPreferences, RouterClientConfig};

/// Identifies a configuration layer, from lowest to highest precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigLayer {
    /// Built-in defaults.
    SystemDefaults,
    /// Global user configuration.
    Global,
    /// Project configuration.
    Project,
    /// Uncommitted local preferences.
    LocalPrefs,
    /// In-memory runtime override (e.g. CLI flags).
    RuntimeOverride,
}

impl ConfigLayer {
    /// Whether this is a *preference* layer, which may only tighten — never
    /// weaken — safety-critical fields set by a config layer.
    #[must_use]
    pub fn is_preference(self) -> bool {
        matches!(self, Self::LocalPrefs | Self::RuntimeOverride)
    }
}

/// Merges `layers` in ascending precedence order into a single [`Config`].
///
/// The input is `(layer, config)` pairs; they are sorted by [`ConfigLayer`]
/// order, then folded low→high so higher layers win. Safety-critical fields are
/// unioned or tighten-only for preference layers, never weakened (see module
/// docs).
///
/// # Example
///
/// Given a `Project` layer that sets `router.url` and a higher `LocalPrefs`
/// layer that only enables `router.auto_start`, the merged config keeps the
/// project's URL *and* has `auto_start` enabled — see the
/// `should_preserve_lower_router_fields_when_higher_sets_one` unit test. (No
/// runnable doc example: this crate is a binary, so it has no library target
/// for doctests to run against.)
#[must_use]
pub fn merge(mut layers: Vec<(ConfigLayer, Config)>) -> Config {
    tracing::trace!(
        merge.layer_count = layers.len(),
        "merging {{merge.layer_count}} config layers"
    );
    layers.sort_by_key(|(layer, _)| *layer);
    let mut acc = Config::default();
    for (layer, config) in layers {
        acc = merge_two(acc, config, layer.is_preference());
    }
    tracing::debug!(
        merge.provider_count = acc.providers.len(),
        merge.rule_count = acc.routing.rules.len(),
        "config layer merge complete"
    );
    acc
}

/// Merges `high` onto `low`. `high_is_preference` marks `high` as a preference
/// layer, which may only tighten safety-critical fields.
fn merge_two(low: Config, high: Config, high_is_preference: bool) -> Config {
    tracing::trace!(
        merge.high_is_preference = high_is_preference,
        "merging one config layer onto the accumulator"
    );
    Config {
        providers: replace_if_non_empty(low.providers, high.providers),
        routing: if high.routing == Default::default() {
            low.routing
        } else {
            high.routing
        },
        classification: if high.classification == Default::default() {
            low.classification
        } else {
            high.classification
        },
        tools: merge_tools(low.tools, high.tools, high_is_preference),
        privacy: merge_privacy(low.privacy, high.privacy, high_is_preference),
        router: merge_router_client(low.router, high.router),
        local: merge_local_prefs(low.local, high.local),
    }
}

/// Returns `high` if it is non-empty, otherwise `low` (replace strategy).
fn replace_if_non_empty<K: Ord, V>(low: BTreeMap<K, V>, high: BTreeMap<K, V>) -> BTreeMap<K, V> {
    if high.is_empty() { low } else { high }
}

/// Merges router client settings field-by-field.
///
/// For optional fields the higher layer's `Some` wins, otherwise the lower
/// value is kept. `auto_start` latches on: once any layer enables it, it stays
/// enabled and a higher layer cannot disable it.
fn merge_router_client(low: RouterClientConfig, high: RouterClientConfig) -> RouterClientConfig {
    RouterClientConfig {
        url: high.url.or(low.url),
        auto_start: high.auto_start || low.auto_start,
        connect_timeout_ms: high.connect_timeout_ms.or(low.connect_timeout_ms),
        request_timeout_ms: high.request_timeout_ms.or(low.request_timeout_ms),
        auth_source: high.auth_source.or(low.auth_source),
    }
}

/// Merges local preferences field-by-field; each higher-layer `Some` wins,
/// otherwise the lower value is kept.
fn merge_local_prefs(low: LocalPreferences, high: LocalPreferences) -> LocalPreferences {
    LocalPreferences {
        auto_apply: high.auto_apply.or(low.auto_apply),
        stream: high.stream.or(low.stream),
        local_only: high.local_only.or(low.local_only),
        no_network: high.no_network.or(low.no_network),
    }
}

/// Merges tool permissions. A config layer overrides per tool; a preference
/// layer may only make a tool's mode stricter, so a lower-layer `deny` is never
/// weakened.
fn merge_tools(low: ToolsConfig, high: ToolsConfig, high_is_preference: bool) -> ToolsConfig {
    tracing::trace!(
        merge.tool_count = high.permissions.len(),
        merge.high_is_preference = high_is_preference,
        "merging {{merge.tool_count}} tool permission overrides"
    );
    let mut merged = low;
    for (tool, mode) in high.permissions {
        match merged.permissions.get(&tool) {
            Some(existing) if high_is_preference => {
                let stricter = (*existing).max(mode);
                if mode < *existing {
                    // A preference layer attempted to weaken a tighter tool mode;
                    // the stricter value is kept (the value itself is not logged).
                    tracing::warn!(
                        merge.tool = %tool,
                        merge.kept_mode = ?stricter,
                        merge.attempted_mode = ?mode,
                        "preference layer attempted to weaken tool {{merge.tool}}; keeping stricter mode"
                    );
                }
                merged.permissions.insert(tool, stricter);
            }
            _ => {
                merged.permissions.insert(tool, mode);
            }
        }
    }
    merged
}

/// Merges privacy policy. Path lists are always unioned (additive). Modes are
/// overridden by config layers but only tightened by preference layers.
fn merge_privacy(
    low: PrivacyPolicy,
    high: PrivacyPolicy,
    high_is_preference: bool,
) -> PrivacyPolicy {
    tracing::trace!(
        merge.high_is_preference = high_is_preference,
        "merging privacy policy"
    );
    let mut merged = low;

    merged.restricted_paths = union_paths(merged.restricted_paths, high.restricted_paths);
    merged.remote.blocked_paths =
        union_paths(merged.remote.blocked_paths, high.remote.blocked_paths);

    let low_remote_mode = merged.remote.mode;
    merged.remote.mode = merge_mode(
        merged.remote.mode,
        high.remote.mode,
        PermissionMode::Ask,
        high_is_preference,
    );
    if high_is_preference
        && let (Some(low_mode), Some(high_mode)) = (low_remote_mode, high.remote.mode)
        && high_mode < low_mode
    {
        // A preference layer attempted to weaken the remote privacy mode; the
        // stricter config-layer value is kept.
        tracing::warn!(
            merge.field = "privacy.remote.mode",
            merge.kept_mode = ?merged.remote.mode,
            merge.attempted_mode = ?high_mode,
            "preference layer attempted to weaken {{merge.field}}; keeping stricter mode"
        );
    }

    let low_local_mode = merged.local.mode;
    merged.local.mode = merge_mode(
        merged.local.mode,
        high.local.mode,
        PermissionMode::Allow,
        high_is_preference,
    );
    if high_is_preference
        && let (Some(low_mode), Some(high_mode)) = (low_local_mode, high.local.mode)
        && high_mode < low_mode
    {
        // A preference layer attempted to weaken the local privacy mode; the
        // stricter config-layer value is kept.
        tracing::warn!(
            merge.field = "privacy.local.mode",
            merge.kept_mode = ?merged.local.mode,
            merge.attempted_mode = ?high_mode,
            "preference layer attempted to weaken {{merge.field}}; keeping stricter mode"
        );
    }
    merged
}

/// Unions two glob lists, sorted and deduplicated.
fn union_paths(mut low: Vec<String>, high: Vec<String>) -> Vec<String> {
    low.extend(high);
    low.sort();
    low.dedup();
    low
}

/// Merges an optional permission mode.
///
/// A config layer's `Some` overrides; a preference layer may only tighten the
/// effective mode (comparing against `default` when the lower layer is unset).
/// `None` in `high` always keeps `low`.
fn merge_mode(
    low: Option<PermissionMode>,
    high: Option<PermissionMode>,
    default: PermissionMode,
    high_is_preference: bool,
) -> Option<PermissionMode> {
    match high {
        None => low,
        Some(high_mode) if high_is_preference => {
            let low_effective = low.unwrap_or(default);
            Some(low_effective.max(high_mode))
        }
        Some(high_mode) => Some(high_mode),
    }
}

#[cfg(test)]
mod tests {
    use smista_sdk::core::policy::{PermissionMode, PrivacyPolicy, RemotePrivacy};

    use super::*;

    fn with_privacy(privacy: PrivacyPolicy) -> Config {
        Config {
            privacy,
            ..Default::default()
        }
    }

    #[test]
    fn should_order_layers_low_to_high() {
        assert!(ConfigLayer::Global < ConfigLayer::Project);
        assert!(ConfigLayer::Project < ConfigLayer::LocalPrefs);
        assert!(ConfigLayer::LocalPrefs < ConfigLayer::RuntimeOverride);
    }

    #[test]
    fn should_classify_preference_layers() {
        assert!(ConfigLayer::LocalPrefs.is_preference());
        assert!(ConfigLayer::RuntimeOverride.is_preference());
        assert!(!ConfigLayer::Project.is_preference());
        assert!(!ConfigLayer::Global.is_preference());
    }

    #[test]
    fn should_let_higher_layer_replace_router_url() {
        let mut low = Config::default();
        low.router.url = Some("low".to_string());
        let mut high = Config::default();
        high.router.url = Some("high".to_string());
        let merged = merge(vec![
            (ConfigLayer::Project, low),
            (ConfigLayer::LocalPrefs, high),
        ]);
        assert_eq!(merged.router.url.as_deref(), Some("high"));
    }

    #[test]
    fn should_apply_layers_regardless_of_input_order() {
        let mut low = Config::default();
        low.router.url = Some("low".to_string());
        let mut high = Config::default();
        high.router.url = Some("high".to_string());
        // Provided high-first; merge must still sort and apply project under local.
        let merged = merge(vec![
            (ConfigLayer::LocalPrefs, high),
            (ConfigLayer::Project, low),
        ]);
        assert_eq!(merged.router.url.as_deref(), Some("high"));
    }

    #[test]
    fn should_union_restricted_paths_across_layers() {
        let project = with_privacy(PrivacyPolicy {
            restricted_paths: vec![".env".to_string()],
            ..Default::default()
        });
        let local = with_privacy(PrivacyPolicy {
            restricted_paths: vec!["secrets.toml".to_string()],
            ..Default::default()
        });
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(
            merged.privacy.restricted_paths,
            vec![".env".to_string(), "secrets.toml".to_string()]
        );
    }

    #[test]
    fn should_not_let_preference_layer_weaken_remote_mode() {
        let project = with_privacy(PrivacyPolicy {
            remote: RemotePrivacy {
                mode: Some(PermissionMode::Deny),
                ..Default::default()
            },
            ..Default::default()
        });
        let local = with_privacy(PrivacyPolicy {
            remote: RemotePrivacy {
                mode: Some(PermissionMode::Allow),
                ..Default::default()
            },
            ..Default::default()
        });
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(
            merged.privacy.remote.mode(),
            PermissionMode::Deny,
            "a preference layer must not weaken the project remote mode"
        );
    }

    #[test]
    fn should_let_config_layer_override_remote_mode() {
        let global = with_privacy(PrivacyPolicy {
            remote: RemotePrivacy {
                mode: Some(PermissionMode::Deny),
                ..Default::default()
            },
            ..Default::default()
        });
        let project = with_privacy(PrivacyPolicy {
            remote: RemotePrivacy {
                mode: Some(PermissionMode::Ask),
                ..Default::default()
            },
            ..Default::default()
        });
        let merged = merge(vec![
            (ConfigLayer::Global, global),
            (ConfigLayer::Project, project),
        ]);
        assert_eq!(
            merged.privacy.remote.mode(),
            PermissionMode::Ask,
            "a project (config) layer may override a global mode"
        );
    }

    #[test]
    fn should_not_let_preference_layer_weaken_a_deny() {
        let mut project = Config::default();
        project.tools.set("shell", PermissionMode::Deny);
        let mut local = Config::default();
        local.tools.set("shell", PermissionMode::Allow);
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(
            merged.tools.mode_for("shell"),
            Some(PermissionMode::Deny),
            "a project Deny must not be weakened by a local Allow"
        );
    }

    #[test]
    fn should_let_config_layer_override_tool_mode() {
        let mut global = Config::default();
        global.tools.set("shell", PermissionMode::Deny);
        let mut project = Config::default();
        project.tools.set("shell", PermissionMode::Ask);
        let merged = merge(vec![
            (ConfigLayer::Global, global),
            (ConfigLayer::Project, project),
        ]);
        assert_eq!(
            merged.tools.mode_for("shell"),
            Some(PermissionMode::Ask),
            "a project (config) layer may override a global tool mode"
        );
    }

    #[test]
    fn should_preserve_lower_router_fields_when_higher_sets_one() {
        let mut project = Config::default();
        project.router.url = Some("low".to_string());
        let mut local = Config::default();
        local.router.auto_start = true;
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(
            merged.router.url.as_deref(),
            Some("low"),
            "a higher layer setting only auto_start must not drop the lower url"
        );
        assert!(merged.router.auto_start);
    }

    #[test]
    fn should_merge_local_prefs_field_wise() {
        let mut project = Config::default();
        project.local.auto_apply = Some(true);
        let mut local = Config::default();
        local.local.stream = Some(true);
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(merged.local.auto_apply, Some(true));
        assert_eq!(merged.local.stream, Some(true));
    }

    #[test]
    fn should_let_higher_layer_add_new_tool_override() {
        let project = Config::default();
        let mut local = Config::default();
        local.tools.set("read_file", PermissionMode::Allow);
        let merged = merge(vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ]);
        assert_eq!(
            merged.tools.mode_for("read_file"),
            Some(PermissionMode::Allow)
        );
    }
}
