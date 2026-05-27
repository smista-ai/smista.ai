//! Request and response types for the router HTTP JSON API.
//!
//! This module is the wire contract for the router's `/api/v1` REST API: the
//! request and response envelopes for authentication, sessions, task execution,
//! route previews, approvals, traces, provider/model listings and usage, plus
//! the structured error body. The router produces these, smista-web serializes
//! them, the Rust client and the TypeScript SDK consume them.
//!
//! The types here are pure, serialization-first value types. They add only the
//! API envelopes and reuse the crate's domain vocabulary wherever the wire
//! shape allows: [`TaskIntent`](crate::intent::TaskIntent),
//! [`ModelReference`](crate::model::ModelReference),
//! [`PermissionMode`](crate::policy::PermissionMode),
//! [`Provider`](crate::model::Provider),
//! [`ProviderDescriptor`](crate::model::ProviderDescriptor),
//! [`Message`](crate::message::Message), [`Usage`](crate::usage::Usage),
//! [`Skill`](crate::skill::Skill) and [`Trace`](crate::trace::Trace).
//!
//! # Conventions
//!
//! All bodies are JSON with `snake_case` field names. Every type derives
//! `Serialize` and `Deserialize` and round-trips through `serde_json`. Optional
//! fields are omitted from output when absent.
//!
//! # Streaming
//!
//! The `POST /sessions/{id}/stream` endpoint takes the same body as
//! [`ExecuteRequest`] and emits a stream of
//! [`StreamEvent`](crate::stream::StreamEvent) values (Server-Sent Events);
//! there is no dedicated streaming envelope here.
//!
//! # Secrets
//!
//! [`BootstrapResponse::api_key`] and [`SignInResponse::token`] are secrets:
//! they appear in responses exactly once and must never be logged, traced or
//! echoed back. Credential headers are handled at the HTTP boundary and never
//! enter these types.

mod approval;
mod auth;
mod error;
mod execute;
mod llm;
mod preview;
mod session;
mod trace;
mod usage;

pub use approval::{ApprovalDecision, SubmitApprovalRequest, SubmitApprovalResponse};
pub use auth::{BootstrapResponse, MeResponse, SignInRequest, SignInResponse, SignOutResponse};
pub use error::{ApiError, ApiErrorBody};
pub use execute::{
    ContextFile, ContextInstruction, ContextOutcome, ExecuteContext, ExecutePermissions,
    ExecutePolicy, ExecutePrivacy, ExecuteRequest, ExecuteResponse, ExecuteRoutingPolicy,
    ExecuteRoutingRule, ExecutionStatus, LocalPreferences, ProviderCredentialInfo,
    ProviderModelInfo, RoutingOutcome, RuleMatch, TaskInput, Workspace,
};
pub use llm::{ListModelsResponse, ListProvidersResponse, ModelInfo};
pub use preview::{CostRange, PreviewResponse, RequiredPermission};
pub use session::{
    CreateSessionRequest, CreateSessionResponse, DeleteSessionResponse, GetSessionResponse,
    SessionDetail, SessionSummary, UpdateSessionRequest,
};
pub use trace::TraceResponse;
pub use usage::{ModelUsage, SessionUsageResponse, TaskTypeUsage, UsageBreakdown};
