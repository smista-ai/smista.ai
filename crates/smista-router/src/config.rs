//! Router runtime configuration (`router.toml`).
//!
//! Unlike the CLI configuration, the router config is a single runtime file with
//! no layered merge. Its subtypes are router-local: they are never exchanged over
//! the API, so they live here rather than in `smista-core`. Path resolution lives
//! in [`paths`].

pub mod paths;

mod load;
mod model;

pub use load::{RouterConfigError, load, parse};
pub use model::{
    CorsConfig, LoggingConfig, OllamaConfig, OllamaLimits, OllamaModels, RetentionConfig,
    RouterAuthConfig, RouterConfig, RouterLimits, StorageConfig, StorageEngine, StorageMode,
};
