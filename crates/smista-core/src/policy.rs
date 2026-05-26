//! Routing and safety policy types.
//!
//! These types are the deterministic policy vocabulary shared across the
//! CLI→router API boundary: routing rules, classification, privacy constraints
//! and tool permissions. They are provider-agnostic and serialization-friendly.
//! The CLI loads them from `config.toml` and sends the relevant subset to the
//! router, which evaluates the *same* types — there is no duplicated wire model.

mod classification;
mod permission;
mod privacy;
mod routing;
mod tools;

pub use classification::{ClassificationConfig, ClassificationRule};
pub use permission::PermissionMode;
pub use privacy::{LocalPrivacy, PrivacyPolicy, RemotePrivacy};
pub use routing::{DefaultRoute, RoutingPolicy, RoutingRule};
pub use tools::ToolsConfig;
