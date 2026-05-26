//! Model and provider domain types.
//!
//! These types form the vocabulary for identifying models, describing what
//! they can do, and checking whether one can serve a given task. They are
//! provider-agnostic and serialization-friendly, and are consumed by routing,
//! storage, providers, trace, web and the SDK.

mod auth;
mod capabilities;
mod descriptor;
mod parameters;
mod provider;
mod reference;

pub use auth::ModelAuthRequirement;
pub use capabilities::{Capability, ModelCapabilities, RoutingRequirements};
pub use descriptor::ModelDescriptor;
pub use parameters::ModelParameters;
pub use provider::{Provider, ProviderDescriptor};
pub use reference::ModelReference;
