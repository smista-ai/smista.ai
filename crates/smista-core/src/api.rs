//! Request and response types for the router HTTP JSON API.
//!
//! This module is the wire contract for the router's `/api/v1` REST API: the
//! request and response envelopes for authentication, sessions, task execution,
//! route previews, approvals, traces, provider/model listings and usage, plus
//! the structured error body. The router produces these, smista-router serializes
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

mod advance;
mod approval;
mod auth;
mod content_ref;
mod crypto;
mod error;
mod execute;
mod llm;
mod preview;
mod session;
mod status;
mod trace;
mod turn_event;
mod usage;

pub use advance::{ApprovalDecisionEntry, ContinueKind, ContinueRequest, ToolResult, UserMessage};
pub use approval::ApprovalDecision;
pub use auth::{BootstrapResponse, MeResponse, SignInResponse, SignOutResponse};
pub use content_ref::ContentRef;
pub use crypto::EncryptedPayload;
pub use error::{ApiError, ApiErrorBody, ApiErrorCode, ApiErrorResponse};
pub use execute::{
    ApprovalKind, Attachments, CompletedTurn, ContextFile, ContextInstruction, ContextOutcome,
    ExecutePolicy, ExecuteRequest, LocalPreferences, PendingApproval, RoutingOutcome, TaskInput,
    ToolApproval, ToolRequest, TurnOutcome, TurnResponse, Workspace,
};
pub use llm::{ListModelsResponse, ListProvidersResponse, UnavailableProvider};
pub use preview::{CostRange, PreviewResponse, RequiredPermission};
pub use session::{
    CreateSessionRequest, CreateSessionResponse, DeleteSessionResponse, GetSessionResponse,
    MessageContent, SessionDetail, SessionMessageDetail, SessionSummary, UpdateSessionRequest,
    UpdateSessionResponse,
};
pub use status::StatusResponse;
pub use trace::TraceResponse;
pub use turn_event::TurnEvent;
pub use usage::{ModelUsage, SessionUsageResponse, TaskTypeUsage, UsageBreakdown};
