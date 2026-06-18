//! `POST /api/v1/sessions/{session_id}/approvals/{approval_id}` — decide on a
//! pending approval.
//!
//! Takes a
//! [`SubmitApprovalRequest`](smista_core::api::SubmitApprovalRequest) carrying
//! the decision and returns a
//! [`SubmitApprovalResponse`](smista_core::api::SubmitApprovalResponse).
//!
//! Scaffolded; the handler is implemented in the approvals issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/approvals/{approval_id}`.
pub(crate) async fn submit_approval() -> WebError {
    WebError::not_implemented()
}
