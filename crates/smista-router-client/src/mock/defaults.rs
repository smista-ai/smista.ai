//! Canned happy-path response bodies for the [`MockRouter`](super::MockRouter).
//!
//! Each function returns one valid, representative instance of an endpoint's
//! success response, built from the `smista-core` API types so it always matches
//! the wire contract. The [`MockRouter`](super::MockRouter) serves these by
//! default; a test asserting on an exact value can call the same function to get
//! the value it should expect back.

use std::collections::BTreeMap;

use smista_core::api::{
    BootstrapResponse, CompletedTurn, ContextOutcome, CostRange, CreateSessionResponse,
    DeleteSessionResponse, GetSessionResponse, ListModelsResponse, ListProvidersResponse,
    ListSessionsResponse, MeResponse, MessageContent, PreviewResponse, RequiredPermission,
    RoutingOutcome, SessionDetail, SessionMessageDetail, SessionSummary, SessionUsageResponse,
    SignInResponse, SignOutResponse, StatusResponse, TraceResponse, TurnOutcome, TurnResponse,
    UnavailableProvider, UpdateSessionResponse,
};
use smista_core::error::ProviderErrorCategory;
use smista_core::intent::TaskIntent;
use smista_core::message::{Message, MessageRole};
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
    ProviderDescriptor,
};
use smista_core::policy::{Classification, Confidence, IntentSource, PermissionMode};
use smista_core::trace::Trace;
use smista_core::usage::Usage;
use uuid::Uuid;

/// A fixed timestamp used for every dated field in the canned responses.
fn timestamp() -> chrono::DateTime<chrono::Utc> {
    "2026-05-25T09:00:00Z"
        .parse()
        .expect("the canned timestamp is valid RFC 3339")
}

/// A representative session summary shared by the session responses.
fn session_summary() -> SessionSummary {
    SessionSummary {
        id: Uuid::nil(),
        title: Some("Refactor auth middleware".to_owned()),
        scope: None,
        encrypted: false,
        key_id: None,
        created_at: timestamp(),
        updated_at: timestamp(),
        archived: false,
    }
}

/// A representative usage breakdown shared by the turn and usage responses.
fn usage() -> Usage {
    Usage {
        input_tokens: Some(1_200),
        output_tokens: Some(500),
        ..Default::default()
    }
}

/// Canned body for `GET /status`.
pub(crate) fn status() -> StatusResponse {
    StatusResponse {
        status: "ok".to_owned(),
        version: "0.0.0".to_owned(),
    }
}

/// Canned body for `POST /api/v1/auth/bootstrap`.
///
/// The API key is well-formed (`sk-smista-api01-<user-id>-<secret>`) so a client
/// that validates it on the way in accepts it.
pub(crate) fn bootstrap() -> BootstrapResponse {
    BootstrapResponse {
        user_id: "user:abc123".to_owned(),
        api_key: format!("sk-smista-api01-{}-examplesecret", Uuid::nil().simple()),
    }
}

/// Canned body for `POST /api/v1/auth/sign-in`.
///
/// The token is well-formed (`<token-id>-<secret>`) so a client that validates
/// it on the way in accepts it.
pub(crate) fn sign_in() -> SignInResponse {
    SignInResponse {
        token: format!("{}-examplesecret", Uuid::nil().simple()),
        expires_at: timestamp(),
    }
}

/// Canned body for `POST /api/v1/auth/sign-out`.
pub(crate) fn sign_out() -> SignOutResponse {
    SignOutResponse { revoked: true }
}

/// Canned body for `GET /api/v1/auth/me`.
pub(crate) fn me() -> MeResponse {
    MeResponse {
        user_id: "user:abc123".to_owned(),
    }
}

/// Canned body for `POST /api/v1/sessions`.
pub(crate) fn create_session() -> CreateSessionResponse {
    CreateSessionResponse {
        session: session_summary(),
    }
}

/// Canned body for `GET /api/v1/sessions`.
pub(crate) fn list_sessions() -> ListSessionsResponse {
    ListSessionsResponse {
        sessions: vec![session_summary()],
    }
}

/// Canned body for `GET /api/v1/sessions/{id}`.
pub(crate) fn get_session() -> GetSessionResponse {
    GetSessionResponse {
        session: SessionDetail {
            id: Uuid::nil(),
            title: "Refactor auth middleware".to_owned(),
            scope: None,
            encrypted: false,
            created_at: timestamp(),
            updated_at: timestamp(),
            messages: vec![
                SessionMessageDetail {
                    role: MessageRole::User,
                    content: MessageContent::Plaintext("Refactor the auth middleware.".to_owned()),
                    provider: None,
                    model: None,
                },
                SessionMessageDetail {
                    role: MessageRole::Assistant,
                    content: MessageContent::Plaintext("Here is the plan...".to_owned()),
                    provider: Some(Provider::Anthropic),
                    model: Some("claude-sonnet".to_owned()),
                },
            ],
            metadata: serde_json::json!({}),
        },
    }
}

/// Canned body for `PUT /api/v1/sessions/{id}`.
pub(crate) fn update_session() -> UpdateSessionResponse {
    UpdateSessionResponse {
        session: session_summary(),
    }
}

/// Canned body for `DELETE /api/v1/sessions/{id}`.
pub(crate) fn delete_session() -> DeleteSessionResponse {
    DeleteSessionResponse { deleted: true }
}

/// A representative classification shared by the turn and preview responses.
fn classification(intent: TaskIntent) -> Classification {
    Classification {
        intent,
        source: IntentSource::Inferred,
        reason: "keyword matched".to_owned(),
        matched_rule: Some(0),
        confidence: Some(Confidence::High),
    }
}

/// Canned body for `POST /api/v1/sessions/{id}/execute` and `/continue`: a
/// completed, plaintext turn.
pub(crate) fn turn() -> TurnResponse {
    TurnResponse {
        outcome: TurnOutcome::Completed(Box::new(CompletedTurn {
            message: Message {
                role: MessageRole::Assistant,
                content: "Here is the refactor.".to_owned(),
                provider: Some(Provider::Anthropic),
                model: Some("claude-sonnet".to_owned()),
            },
            classification: classification(TaskIntent::Edit),
            routing: RoutingOutcome {
                task_type: TaskIntent::Edit,
                provider: Provider::Anthropic,
                model: "claude-sonnet".to_owned(),
                matched_rule: Some("edit + src/auth/** -> anthropic/claude-sonnet".to_owned()),
                fallback_used: false,
                override_used: false,
            },
            context: ContextOutcome {
                included: vec!["src/auth/middleware.rs".to_owned()],
                excluded: vec![".env".to_owned()],
            },
            usage: usage(),
            to_encrypt: BTreeMap::new(),
            trace_id: "trace:xyz".to_owned(),
        })),
        allowed_continuations: Vec::new(),
    }
}

/// Canned body for `POST /api/v1/sessions/{id}/preview`.
pub(crate) fn preview() -> PreviewResponse {
    PreviewResponse {
        task_type: TaskIntent::Review,
        classification: classification(TaskIntent::Review),
        provider: Provider::OpenAI,
        model: "gpt-5.5-thinking".to_owned(),
        matched_rule: Some("task.review -> openai/gpt-5.5-thinking".to_owned()),
        included_context: vec!["current git diff".to_owned(), "AGENTS.md".to_owned()],
        excluded_context: vec![".env".to_owned()],
        estimated_cost: CostRange {
            min: rust_decimal::Decimal::new(3, 2),
            max: rust_decimal::Decimal::new(9, 2),
            currency: "USD".to_owned(),
        },
        required_permissions: vec![
            RequiredPermission {
                permission: "read_repository".to_owned(),
                mode: PermissionMode::Allow,
            },
            RequiredPermission {
                permission: "write_files".to_owned(),
                mode: PermissionMode::Ask,
            },
        ],
    }
}

/// Canned body for `GET /api/v1/sessions/{id}/traces`: an empty trace.
pub(crate) fn traces() -> TraceResponse {
    TraceResponse {
        trace: Trace {
            session_id: Uuid::nil(),
            events: Vec::new(),
        },
    }
}

/// Canned body for `GET /api/v1/llm/providers`.
pub(crate) fn list_providers() -> ListProvidersResponse {
    ListProvidersResponse {
        providers: vec![ProviderDescriptor {
            id: Provider::OpenAI,
            display_name: "OpenAI".to_owned(),
            local: false,
        }],
    }
}

/// Canned body for `GET /api/v1/llm/models`.
pub(crate) fn list_models() -> ListModelsResponse {
    ListModelsResponse {
        models: vec![ModelDescriptor {
            provider: Provider::Ollama,
            model: "qwen2.5-coder".to_owned(),
            display_name: None,
            local: true,
            auth: ModelAuthRequirement::None,
            capabilities: ModelCapabilities {
                streaming: true,
                ..Default::default()
            },
            max_context_tokens: 32_768,
            max_output_tokens: None,
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }],
        unavailable: vec![UnavailableProvider {
            provider: Provider::Anthropic,
            reason: ProviderErrorCategory::MissingCredentials,
            message: None,
        }],
    }
}

/// Canned body for `GET /api/v1/sessions/{id}/usage`.
pub(crate) fn session_usage() -> SessionUsageResponse {
    SessionUsageResponse {
        total: usage(),
        by_model: Vec::new(),
        by_task_type: Vec::new(),
    }
}
