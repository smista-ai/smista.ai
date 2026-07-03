use crate::{ResponseTemplate, defaults};

/// HTTP status returned by a successful creation, per the router.
const CREATED: u16 = 201;
/// HTTP status returned by a successful read or update, per the router.
const OK: u16 = 200;

/// One router endpoint the [`MockRouter`](crate::MockRouter) serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// `GET /status`.
    Status,
    /// `POST /api/v1/auth/bootstrap`.
    Bootstrap,
    /// `POST /api/v1/auth/sign-in`.
    SignIn,
    /// `POST /api/v1/auth/sign-out`.
    SignOut,
    /// `GET /api/v1/auth/me`.
    Me,
    /// `POST /api/v1/sessions`.
    CreateSession,
    /// `GET /api/v1/sessions`.
    ListSessions,
    /// `GET /api/v1/sessions/{id}`.
    GetSession,
    /// `PUT /api/v1/sessions/{id}`.
    UpdateSession,
    /// `DELETE /api/v1/sessions/{id}`.
    DeleteSession,
    /// `POST /api/v1/sessions/{id}/execute`.
    Execute,
    /// `POST /api/v1/sessions/{id}/continue`.
    ContinueRun,
    /// `POST /api/v1/sessions/{id}/preview`.
    Preview,
    /// `GET /api/v1/sessions/{id}/traces`.
    GetTraces,
    /// `GET /api/v1/llm/providers`.
    ListProviders,
    /// `GET /api/v1/llm/models`.
    ListModels,
    /// `GET /api/v1/sessions/{id}/usage`.
    SessionUsage,
}

impl Endpoint {
    /// Returns whether this endpoint has a documented not-found resource case.
    pub(crate) const fn allows_not_found(self) -> bool {
        matches!(
            self,
            Self::GetSession
                | Self::UpdateSession
                | Self::DeleteSession
                | Self::Execute
                | Self::ContinueRun
                | Self::Preview
                | Self::GetTraces
                | Self::SessionUsage
        )
    }

    /// The canned happy-path response this endpoint serves by default.
    pub(crate) fn default_response(self) -> ResponseTemplate {
        match self {
            Self::Status => ResponseTemplate::new(OK).set_body_json(defaults::status()),
            Self::Bootstrap => ResponseTemplate::new(CREATED).set_body_json(defaults::bootstrap()),
            Self::SignIn => ResponseTemplate::new(OK).set_body_json(defaults::sign_in()),
            Self::SignOut => ResponseTemplate::new(OK).set_body_json(defaults::sign_out()),
            Self::Me => ResponseTemplate::new(OK).set_body_json(defaults::me()),
            Self::CreateSession => {
                ResponseTemplate::new(CREATED).set_body_json(defaults::create_session())
            }
            Self::ListSessions => {
                ResponseTemplate::new(OK).set_body_json(defaults::list_sessions())
            }
            Self::GetSession => ResponseTemplate::new(OK).set_body_json(defaults::get_session()),
            Self::UpdateSession => {
                ResponseTemplate::new(OK).set_body_json(defaults::update_session())
            }
            Self::DeleteSession => {
                ResponseTemplate::new(OK).set_body_json(defaults::delete_session())
            }
            Self::Execute | Self::ContinueRun => {
                ResponseTemplate::new(OK).set_body_json(defaults::turn())
            }
            Self::Preview => ResponseTemplate::new(OK).set_body_json(defaults::preview()),
            Self::GetTraces => ResponseTemplate::new(OK).set_body_json(defaults::traces()),
            Self::ListProviders => {
                ResponseTemplate::new(OK).set_body_json(defaults::list_providers())
            }
            Self::ListModels => ResponseTemplate::new(OK).set_body_json(defaults::list_models()),
            Self::SessionUsage => {
                ResponseTemplate::new(OK).set_body_json(defaults::session_usage())
            }
        }
    }
}

/// The response class served for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointStatus {
    /// Return the configured success response.
    Ok,
    /// Return `401 Unauthorized`.
    Unauthorized,
    /// Return `500 Internal Server Error`.
    ServerError,
    /// Return `404 Not Found`.
    NotFound,
}
