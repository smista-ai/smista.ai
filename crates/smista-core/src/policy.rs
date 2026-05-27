//! Routing and safety policy types.
//!
//! These types are the deterministic policy vocabulary shared across the
//! CLI→router API boundary: routing rules, classification, privacy constraints
//! and tool permissions. They are provider-agnostic and serialization-friendly.
//! The CLI loads them from `config.toml` and sends the relevant subset to the
//! router, which evaluates the *same* types — there is no duplicated wire model.

mod classification;
mod glob;
mod permission;
mod privacy;
mod routing;
mod tools;

pub use classification::{
    Classification, ClassificationConfig, ClassificationRule, Confidence, IntentSource,
};
pub use permission::PermissionMode;
pub use privacy::{LocalPrivacy, PrivacyPolicy, RemotePrivacy};
pub use routing::{DefaultRoute, RoutingContext, RoutingPolicy, RoutingRule, Specificity};
pub use tools::ToolsConfig;
