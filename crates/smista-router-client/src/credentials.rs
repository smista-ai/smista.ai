//! Router authentication credentials presented by the client.
//!
//! Both the [`ApiKey`] exchanged at sign-in and the [`SessionToken`] sent on
//! every authenticated request are defined in [`smista_core::credential`], the
//! single source of truth for their on-the-wire format shared with the router.
//! They are re-exported here so client callers reach them through this crate.

mod provider_credentials;

#[doc(inline)]
pub use smista_core::credential::{ApiKey, SessionToken};

#[doc(inline)]
pub use self::provider_credentials::ProviderCredentials;
