//! The execution orchestrator: the per-turn run loop behind `execute`,
//! `stream` and `continue`.
//!
//! The orchestrator is a stateless-between-turns driver. Each accepted request
//! acquires the run lock, runs the turn loop (classify, resolve, invoke,
//! mediate) by reusing the deterministic [`Resolver`](crate::router::resolver),
//! persists the produced work, writes the next durable phase, and releases the
//! lock. Routing never depends on an LLM; the orchestrator only feeds the
//! resolver inputs it recalls from storage and acts on its output.
#![allow(
    dead_code,
    reason = "the orchestrator is mounted on the web execute route in a later task"
)]

mod cost;
mod crypto;
mod error;
mod invoke;
mod mediation;
mod persist;
mod preview;
mod prompt;
mod recall;
mod registry;
mod run_input;
mod stream;
#[cfg(test)]
mod tests;
mod tools;
mod trace_emit;
mod turn;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use secrecy::SecretString;
use smista_core::api::{
    ApprovalKind, CompletedTurn, ContentRef, ContextOutcome, ContinueKind, ContinueRequest,
    EncryptedPayload, ExecuteRequest, PendingApproval, PreviewResponse, RoutingOutcome, ToolResult,
    TurnOutcome, TurnResponse, UserMessage,
};
use smista_core::message::{Message, MessageRole};
use smista_core::model::{Provider, RoutingRequirements};
use smista_providers::api::RequestMessage;
use smista_providers::memory::MemoryScope;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{
    ActiveTurn, ApprovalKind as StorageApprovalKind, PendingWrite, ResumeStep, RunPhase, RunState,
    ToolCallStatus,
};
use smista_storage::surrealdb::RecordId;
use smista_storage::types::SecretContent;
use uuid::Uuid;

pub(crate) use self::error::OrchestratorError;
use self::persist::{
    persist_approval, persist_context_references, persist_routing_decision, persist_run_input,
    write_sealed_message, write_sealed_plan, write_sealed_tool_call,
};
use self::preview::preview_response;
use self::recall::{Recalled, recall};
use self::registry::{InFlightRegistry, TurnToken};
use self::run_input::{
    RunInputBundle, RunInputMeta, rebuild_run_meta, rebuild_workspace, split_execute_request,
};
pub(crate) use self::stream::TurnSink;
use self::turn::{
    AwaitingApprovalData, AwaitingToolData, BundleSource, CATALOG_TIMEOUT, CompletedData, TurnCx,
    TurnStep, run_turn, to_resolver_attachments, to_resolver_input, to_resolver_workspace,
};
use crate::router::Router;
use crate::router::resolver::context::{RecalledContext, ResolvedContext};
use crate::router::resolver::{ResolveArgs, Resolver, RoutingDecision};
use crate::session::{Sessions, UserSession};

/// Whether a continuation supersedes the in-flight turn instead of being
/// rejected by it. Only `break` and `inject` supersede; every other
/// continuation must find the run lock free.
fn supersedes(kind: ContinueKind) -> bool {
    matches!(kind, ContinueKind::Break | ContinueKind::Inject)
}

/// The shared context threaded through a continuation's phase handlers.
///
/// Built once in [`Orchestrator::advance`] from the opened session, the recalled
/// run state and request snapshot, the caller's credentials and the live turn
/// token, then passed to each handler so it carries only its own payload
/// alongside it. Every field is cheap to copy, so a handler that needs a tweaked
/// view (an injected bundle, a cleared plan flag) derives one with `..*cx`.
#[derive(Clone, Copy)]
struct ContinuationCx<'a> {
    /// The opened session the continuation runs against.
    session: &'a UserSession,
    /// The durable run state recalled for the run being advanced.
    state: &'a RunState,
    /// The run's identifier, parsed from [`RunState::run_id`].
    run_id: Uuid,
    /// The request metadata recalled for the run.
    meta: &'a RunInputMeta,
    /// The run's sealable request bundle: the stored content row by default, or
    /// an injected in-memory bundle when an `inject` supersedes the turn.
    bundle: BundleSource<'a>,
    /// Whether the run is in plan mode.
    plan_active: bool,
    /// The caller's per-provider credentials for this turn.
    credentials: &'a HashMap<Provider, SecretString>,
    /// The in-flight turn token, for supersede and lock release.
    token: &'a TurnToken,
    /// The live event sink for a streamed continuation, threaded into each turn.
    sink: Option<&'a TurnSink>,
}

impl ContinuationCx<'_> {
    /// The owning user's id, taken from the bound session.
    fn user_id(&self) -> Uuid {
        self.session.user_id()
    }

    /// The session's id, taken from the bound session.
    fn session_id(&self) -> Uuid {
        self.session.session_id()
    }
}

/// Drives a session's run loop: admission, the turn pipeline, persistence and
/// lock release.
///
/// Holds the long-lived collaborators shared across requests — the storage
/// handle, the provider registry, the deterministic resolver and the in-flight
/// registry that powers supersede — and is cheap to clone-by-reference.
pub struct Orchestrator {
    /// The storage handle every session and run row is read and written through.
    database: SurrealDatabase,
    /// The provider registry the chosen model is resolved through.
    router: Arc<Router>,
    /// The deterministic, LLM-free routing pipeline, reused verbatim.
    resolver: Arc<Resolver>,
    /// The in-process map of the live turn per session, for supersede.
    registry: InFlightRegistry,
}

impl Orchestrator {
    /// Builds an orchestrator over the given storage, router and resolver.
    #[must_use]
    pub fn new(database: SurrealDatabase, router: Arc<Router>, resolver: Arc<Resolver>) -> Self {
        Self {
            database,
            router,
            resolver,
            registry: InFlightRegistry::default(),
        }
    }

    /// Runs one `execute` turn end to end and returns its outcome.
    ///
    /// Opens the session, admits the request against the run lock, persists the
    /// run-input snapshot, runs the turn loop and persists what it produces, then
    /// releases the lock. A failure anywhere releases the lock before returning.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError`] when the session cannot be opened, a turn is
    /// already in flight, the resolver finds no route, the provider call fails,
    /// or persistence fails.
    pub async fn execute(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        request: ExecuteRequest,
        credentials: HashMap<Provider, SecretString>,
    ) -> Result<TurnResponse, OrchestratorError> {
        self.execute_streaming(user_id, session_id, request, credentials, None)
            .await
    }

    /// Runs one `execute` turn, optionally streaming live events to `sink`.
    ///
    /// Identical to [`Self::execute`] but, when `sink` is set and the model can
    /// stream, drives the turn from the model's live output, forwarding text and
    /// reasoning deltas as they arrive.
    ///
    /// The whole turn runs inside a tracing span carrying the session and (once
    /// admitted) run id, so every event the turn loop emits — down through
    /// resolve, invoke and persistence — is attributable without each call
    /// restating them. `skip_all` keeps the request and credentials out of the
    /// span fields, so no secret is ever recorded.
    #[tracing::instrument(
        skip_all,
        fields(session_id = %session_id, run_id = tracing::field::Empty)
    )]
    pub async fn execute_streaming(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        request: ExecuteRequest,
        credentials: HashMap<Provider, SecretString>,
        sink: Option<TurnSink>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let sessions = Sessions::new(self.database.clone(), user_id);
        let session = sessions.open(session_id).await?;

        let (run_id, token) = self.acquire(&session, user_id, session_id).await?;
        tracing::Span::current().record("run_id", tracing::field::display(run_id));
        tracing::debug!(%session_id, %run_id, "admitted execute request; lock acquired");

        let scope = MemoryScope {
            user_id,
            session_id,
        };
        let (meta, bundle) = split_execute_request(request);
        let encrypted = is_encrypted(&session).await?;
        if let Err(error) = persist_run_input(
            &self.database,
            user_id,
            session_id,
            run_id,
            &meta,
            &bundle,
            encrypted,
        )
        .await
        {
            self.abort(&session, user_id, session_id, run_id, &token)
                .await;
            return Err(error.into());
        }

        let plan_active = matches!(
            bundle.input.command,
            Some(smista_core::intent::TaskIntent::Plan)
        );
        let decrypted = BTreeMap::new();
        let cx = TurnCx {
            router: &self.router,
            resolver: &self.resolver,
            session: &session,
            credentials: &credentials,
            scope,
            cancel: &token,
            meta: &meta,
            bundle: BundleSource::Loaded(&bundle),
            seal_run_input: true,
            plan_active,
            encrypted,
            decrypted: &decrypted,
            sink: sink.as_ref(),
        };
        let step = run_turn(&cx, ResumeStep::BuildPrompt, Vec::new()).await;
        let result = self
            .finish_step(&session, user_id, session_id, run_id, step)
            .await;
        self.settle(result, &session, run_id, &token, None).await
    }

    /// Previews how a turn would be routed, without ever calling the model.
    ///
    /// Opens the session, recalls its plaintext context, and runs the same
    /// deterministic resolve an `execute` turn would, then maps the resolved
    /// plan onto a [`PreviewResponse`]. It acquires no run lock and persists
    /// nothing: a preview neither admits a run nor mutates session state, so it
    /// can run while a turn is in flight and never spends a token.
    ///
    /// An encrypted session whose history is still sealed cannot be opened in a
    /// single request, so the preview resolves over empty recall: the routing
    /// decision is driven by the request, not by the sealed history.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError`] when the session cannot be opened — an
    /// unknown, archived or non-owned session is reported as not found — or the
    /// resolver finds no usable route for the request.
    #[tracing::instrument(skip_all, fields(session_id = %session_id))]
    pub async fn preview(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        request: ExecuteRequest,
        credentials: HashMap<Provider, SecretString>,
    ) -> Result<PreviewResponse, OrchestratorError> {
        let sessions = Sessions::new(self.database.clone(), user_id);
        let session = sessions.open(session_id).await?;

        let (meta, bundle) = split_execute_request(request);

        // A preview never pauses to decrypt: a still-sealed history previews over
        // empty recall, since the routing decision is driven by the request.
        let recalled = match recall(&session, &BTreeMap::new()).await? {
            Recalled::Ready(recalled) => recalled,
            Recalled::NeedsDecrypt(_) => {
                tracing::debug!("session history is sealed; previewing over empty recall");
                RecalledContext::default()
            }
        };

        // The catalog is read with the supplied credentials so model selection
        // sees exactly the models the turn would; no provider request is made
        // beyond listing, and the chosen model is never invoked.
        let catalog = self
            .router
            .fetch_models(credentials.clone(), CATALOG_TIMEOUT)
            .await
            .models;
        let credentialed: HashSet<Provider> = credentials.keys().cloned().collect();

        let workspace = rebuild_workspace(&meta, &bundle);
        let resolver_workspace = to_resolver_workspace(&workspace);
        let resolver_attachments = to_resolver_attachments(&bundle.attachments);
        let resolver_input = to_resolver_input(&bundle.input);

        let resolved = self.resolver.resolve(ResolveArgs {
            input: &resolver_input,
            workspace: &resolver_workspace,
            attachments: &resolver_attachments,
            recalled: &recalled,
            classification: &meta.policy.classification,
            routing: &meta.policy.routing,
            privacy: &meta.policy.privacy,
            requirements: RoutingRequirements::default(),
            catalog: &catalog,
            credentialed: &credentialed,
            local_only: meta.local_preferences.local_only,
        })?;

        tracing::debug!(
            provider = %resolved.routing.provider,
            model = %resolved.routing.model,
            "previewed the route without invoking the model"
        );
        Ok(preview_response(
            &resolved,
            &bundle.input,
            &meta.policy.tools,
        ))
    }

    /// Advances an in-flight run with the client's continuation.
    ///
    /// Opens the session, admits the continuation against the run lock, recalls
    /// the run's request context, and dispatches on the durable phase: tool
    /// results and approvals re-enter the turn loop, while `break` and `inject`
    /// supersede the live turn. The lock is released at the next checkpoint, and
    /// any failure releases it before returning.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorError`] when the session cannot be opened, no run is
    /// in progress, the continuation does not answer the current pause, or a
    /// turn fails.
    pub async fn advance(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        continuation: ContinueRequest,
        credentials: HashMap<Provider, SecretString>,
    ) -> Result<TurnResponse, OrchestratorError> {
        self.advance_streaming(user_id, session_id, continuation, credentials, None)
            .await
    }

    /// Advances an in-flight run, optionally streaming live events to `sink`.
    ///
    /// Identical to [`Self::advance`] but, when `sink` is set and the resumed
    /// turn's model can stream, drives it from the model's live output.
    ///
    /// Runs inside a tracing span carrying the session and (once recalled) run
    /// id, so every event the continuation's turn loop emits is attributable
    /// without each call restating them. `skip_all` keeps the continuation
    /// payload and credentials out of the span fields, so no secret is recorded.
    #[tracing::instrument(
        skip_all,
        fields(session_id = %session_id, run_id = tracing::field::Empty)
    )]
    pub async fn advance_streaming(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        continuation: ContinueRequest,
        credentials: HashMap<Provider, SecretString>,
        sink: Option<TurnSink>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let sessions = Sessions::new(self.database.clone(), user_id);
        let session = sessions.open(session_id).await?;

        let kind = continue_kind(&continuation);

        let Some(state) = session.run_state().await? else {
            tracing::warn!(%session_id, "rejecting continuation: no run is in progress");
            return Err(OrchestratorError::NoActiveRun);
        };
        let run_id = Uuid::parse_str(&state.run_id)
            .map_err(|_| OrchestratorError::Internal("stored run id is not a uuid".to_string()))?;
        tracing::Span::current().record("run_id", tracing::field::display(run_id));

        let (input, content) = session
            .run_input()
            .await?
            .ok_or_else(|| OrchestratorError::Internal("run input is missing".to_string()))?;
        let plan_active = input.plan_active;
        // Only the non-secret meta is rebuilt up front; the bundle stays in its
        // stored content row and the turn opens it (decrypting if sealed) lazily.
        let meta = rebuild_run_meta(&input)?;

        let token = if supersedes(kind) {
            // `break`/`inject` seize the run: a fresh token supersedes the live
            // turn (the in-process half of the supersede rule) and the lock is
            // taken even while a turn holds it.
            let token = self.registry.begin(session_id);
            if let Err(error) = self.mark_active(&session, &state).await {
                self.abort(&session, user_id, session_id, run_id, &token)
                    .await;
                return Err(error);
            }
            token
        } else {
            // Every other continuation must find the lock free. Acquiring it
            // atomically — checking and writing in one transaction — closes the
            // window where two concurrent continuations both pass the check and
            // both proceed. The checkpoint phase is preserved; only `active` is
            // set, so the pause the continuation answers stays intact.
            let mut held = state.clone();
            held.active = Some(ActiveTurn {
                started_at: Utc::now(),
                lease: state.run_id.clone(),
            });
            if session.acquire_run_lock(held).await?.is_none() {
                tracing::warn!(%session_id, "rejecting continuation: a turn is already in flight");
                return Err(OrchestratorError::Busy);
            }
            self.registry.begin(session_id)
        };
        tracing::debug!(%session_id, %run_id, "admitted continuation; lock acquired");

        let cx = ContinuationCx {
            session: &session,
            state: &state,
            run_id,
            meta: &meta,
            bundle: BundleSource::Stored(&content.content),
            plan_active,
            credentials: &credentials,
            token: &token,
            sink: sink.as_ref(),
        };
        // A rejected continuation falls back to this checkpoint rather than
        // resetting the run to idle, so the pause stays answerable.
        let checkpoint = (state.turn, state.phase.clone());
        let result = self.dispatch_continuation(&cx, continuation).await;
        self.settle(result, &session, run_id, &token, Some(checkpoint))
            .await
    }

    /// Routes a continuation to the handler for the run's current phase.
    async fn dispatch_continuation(
        &self,
        cx: &ContinuationCx<'_>,
        continuation: ContinueRequest,
    ) -> Result<TurnResponse, OrchestratorError> {
        match continuation {
            ContinueRequest::ToolResults { results, encrypted } => {
                self.resume_tool_results(cx, results, encrypted).await
            }
            ContinueRequest::ApprovalDecisions {
                decisions,
                encrypted,
            } => self.resume_approvals(cx, decisions, encrypted).await,
            ContinueRequest::Inject { messages } => self.resume_inject(cx, messages).await,
            ContinueRequest::Decrypted {
                plaintext,
                encrypted,
            } => self.resume_decrypted(cx, plaintext, encrypted).await,
            ContinueRequest::Sealed { encrypted } => self.resume_sealed(cx, encrypted).await,
            ContinueRequest::Break => self.resume_break(cx).await,
        }
    }

    /// Ingests tool results, records them, and runs the next turn.
    ///
    /// Each result is matched to the outstanding [`ToolWait`] and any folded
    /// approval recorded. For a plaintext run the tool-call row is moved to a
    /// terminal status with its output stored. For an encrypted run nothing was
    /// stored at request time: the deferred assistant and user messages are
    /// written from the client's `sealed` map, and each tool-call row is written
    /// with its client-sealed result. The results are then handed to the next
    /// turn as followups so the model sees them.
    async fn resume_tool_results(
        &self,
        cx: &ContinuationCx<'_>,
        results: Vec<ToolResult>,
        sealed: BTreeMap<ContentRef, EncryptedPayload>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let session = cx.session;
        let user_id = cx.user_id();
        let session_id = cx.session_id();
        let RunPhase::AwaitingTool { calls, pending, .. } = &cx.state.phase else {
            tracing::warn!(%session_id, "tool results arrived but the run is not awaiting tools");
            return Err(OrchestratorError::UnexpectedContinuation);
        };
        let waiting: std::collections::HashSet<&str> =
            calls.iter().map(|wait| wait.call_id.as_str()).collect();

        // The results must answer every outstanding call exactly once. A missing
        // answer would advance the run with a call still pending; a duplicate
        // would record one twice. Reject either before any row is written.
        let submitted: Vec<&str> = results
            .iter()
            .map(|result| result.call_id.as_str())
            .collect();
        let answered: std::collections::HashSet<&str> = submitted.iter().copied().collect();
        if submitted.len() != answered.len() || answered != waiting {
            tracing::warn!(%session_id, "tool results do not answer the pending calls one-to-one");
            return Err(OrchestratorError::UnexpectedContinuation);
        }

        let encrypted = !pending.is_empty();

        // For an encrypted run the assistant and user messages were deferred;
        // write them now, sealed by the client and returned in `sealed`. The
        // run-input bundle, deferred at run start, is sealed in place here too.
        if encrypted {
            validate_required_seals(session, pending, &sealed).await?;
            flush_pending_messages(session, user_id, session_id, pending, &sealed).await?;
            reseal_run_input(session, &sealed).await?;
        }

        let mut followups = Vec::with_capacity(results.len());
        for result in results {
            let call_uuid = Uuid::parse_str(&result.call_id).map_err(|_| {
                OrchestratorError::Internal("tool call id is not a uuid".to_string())
            })?;
            let status = if result.is_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };
            if encrypted {
                let tool_name = pending_tool_name(pending, &result.call_id).ok_or_else(|| {
                    OrchestratorError::Internal("tool result for an unrecorded call".to_string())
                })?;
                let content =
                    sealed_content(&sealed, &ContentRef::ToolCall(result.call_id.clone()))?;
                let (stored_result, stored_error) = if result.is_error {
                    (None, Some(content))
                } else {
                    (Some(content), None)
                };
                write_sealed_tool_call(
                    session,
                    user_id,
                    session_id,
                    call_uuid,
                    &tool_name,
                    status,
                    stored_result,
                    stored_error,
                )
                .await?;
            } else {
                let (stored_result, stored_error) = if result.is_error {
                    (None, Some(SecretContent::plaintext(result.content.clone())))
                } else {
                    (Some(SecretContent::plaintext(result.content.clone())), None)
                };
                session
                    .set_tool_call_outcome(call_uuid, status, stored_result, stored_error)
                    .await?;
            }

            if let Some(decision) = result.decision {
                persist_approval(
                    session,
                    user_id,
                    session_id,
                    "tool_call",
                    &result.call_id,
                    decision,
                    None,
                )
                .await?;
                tracing::debug!(%session_id, "recorded a folded tool approval");
            }

            followups.push(RequestMessage::ToolResult {
                call_id: result.call_id,
                content: result.content,
                is_error: result.is_error,
            });
        }

        let step = self
            .run_continuation_turn(cx, ResumeStep::NextTurn, followups, &BTreeMap::new())
            .await?;
        self.finish_step(session, user_id, session_id, cx.run_id, step)
            .await
    }

    /// Supersedes the live turn with injected user input and runs a new turn.
    ///
    /// Any outstanding tool calls are cancelled, the injected text becomes the
    /// turn's input, and the loop re-enters from the top so the new message is
    /// recorded against the route that serves it.
    async fn resume_inject(
        &self,
        cx: &ContinuationCx<'_>,
        messages: Vec<UserMessage>,
    ) -> Result<TurnResponse, OrchestratorError> {
        tracing::info!(session_id = %cx.session_id(), "injecting user input and superseding the live turn");
        self.cancel_outstanding_tools(cx.session, cx.state).await?;

        let injected = messages
            .into_iter()
            .map(|message| message.text)
            .collect::<Vec<_>>()
            .join("\n");
        let bundle = RunInputBundle::for_injection(injected);

        // The injected bundle replaces the recalled one for this turn only; it is
        // not persisted, so the run's stored request is left intact.
        let injected_cx = ContinuationCx {
            bundle: BundleSource::Loaded(&bundle),
            ..*cx
        };
        let step = self
            .run_continuation_turn(
                &injected_cx,
                ResumeStep::BuildPrompt,
                Vec::new(),
                &BTreeMap::new(),
            )
            .await?;
        self.finish_step(cx.session, cx.user_id(), cx.session_id(), cx.run_id, step)
            .await
    }

    /// Aborts the live turn with no further input, returning the run to idle.
    async fn resume_break(
        &self,
        cx: &ContinuationCx<'_>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let session = cx.session;
        let state = cx.state;
        tracing::info!(session_id = %cx.session_id(), "breaking the live turn back to idle");
        self.cancel_outstanding_tools(session, state).await?;
        self.release(
            session,
            cx.user_id(),
            cx.session_id(),
            cx.run_id,
            state.turn,
            RunPhase::Idle,
        )
        .await?;
        Ok(TurnResponse {
            outcome: TurnOutcome::Idle {
                trace_id: String::new(),
            },
            allowed_continuations: Vec::new(),
        })
    }

    /// Resumes a run whose sealed history the client has now opened.
    ///
    /// Reaches here only from `AwaitingDecrypt`. The opened `plaintext` is threaded
    /// into recall so the turn builds its prompt and proceeds. When the authoring
    /// turn deferred its run-input bundle to this pause, the client returns it
    /// sealed in `encrypted`; that ciphertext is written over the bundle's
    /// placeholder before the turn resumes.
    async fn resume_decrypted(
        &self,
        cx: &ContinuationCx<'_>,
        plaintext: BTreeMap<ContentRef, String>,
        encrypted: BTreeMap<ContentRef, EncryptedPayload>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let RunPhase::AwaitingDecrypt { resume, .. } = &cx.state.phase else {
            tracing::warn!(session_id = %cx.session_id(), "decrypted continuation arrived but the run is not awaiting decryption");
            return Err(OrchestratorError::UnexpectedContinuation);
        };
        let resume = *resume;
        validate_required_seals(cx.session, &[], &encrypted).await?;
        reseal_run_input(cx.session, &encrypted).await?;
        let step = self
            .run_continuation_turn(cx, resume, Vec::new(), &plaintext)
            .await?;
        self.finish_step(cx.session, cx.user_id(), cx.session_id(), cx.run_id, step)
            .await
    }

    /// Writes the router-authored content the client has now sealed and idles.
    ///
    /// Answers an `AwaitingEncrypt` pause (a completed encrypted turn): the
    /// deferred message and plan rows are written, paired with their ciphertext,
    /// and the run goes idle.
    async fn resume_sealed(
        &self,
        cx: &ContinuationCx<'_>,
        sealed: BTreeMap<ContentRef, EncryptedPayload>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let session = cx.session;
        let user_id = cx.user_id();
        let session_id = cx.session_id();
        let run_id = cx.run_id;
        let RunPhase::AwaitingEncrypt { pending, .. } = &cx.state.phase else {
            tracing::warn!(%session_id, "sealed continuation arrived but the run is not awaiting encryption");
            return Err(OrchestratorError::UnexpectedContinuation);
        };
        validate_required_seals(session, pending, &sealed).await?;
        flush_pending_messages(session, user_id, session_id, pending, &sealed).await?;
        write_pending_plan(session, user_id, session_id, pending, &sealed).await?;
        // Session memory written in clear during the run is sealed in place; its
        // rows already exist, so the ciphertext overwrites their content.
        reseal_memory(session, &sealed).await?;
        // The run-input bundle is sealed in place over its placeholder.
        reseal_run_input(session, &sealed).await?;
        self.release(
            session,
            user_id,
            session_id,
            run_id,
            cx.state.turn,
            RunPhase::Idle,
        )
        .await?;
        tracing::debug!(%session_id, %run_id, "sealed router content written; run idle");
        Ok(idle_response())
    }

    /// Records an approval decision and resumes or finalizes the run.
    ///
    /// Accepting a plan moves it to [`PlanStatus::Approved`], leaves plan mode by
    /// clearing the persisted flag, and runs the next (non-planning) turn;
    /// rejecting it moves the plan to [`PlanStatus::Rejected`] and returns the run
    /// to idle. Disclosure and cost gates invoke on accept and idle on reject.
    async fn resume_approvals(
        &self,
        cx: &ContinuationCx<'_>,
        decisions: Vec<smista_core::api::ApprovalDecisionEntry>,
        sealed: BTreeMap<ContentRef, EncryptedPayload>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let session = cx.session;
        let user_id = cx.user_id();
        let session_id = cx.session_id();
        let run_id = cx.run_id;
        // An approval resumes a non-planning turn, so drop plan mode for the run.
        let resumed_cx = ContinuationCx {
            plan_active: false,
            ..*cx
        };
        let RunPhase::AwaitingApproval {
            approval_id,
            kind,
            detail,
            pending,
            ..
        } = &cx.state.phase
        else {
            tracing::warn!(%session_id, "an approval arrived but the run is not awaiting one");
            return Err(OrchestratorError::UnexpectedContinuation);
        };
        let Some(entry) = decisions
            .into_iter()
            .find(|entry| &entry.approval_id == approval_id)
        else {
            tracing::warn!(%session_id, "no decision answered the pending approval");
            return Err(OrchestratorError::UnexpectedContinuation);
        };
        let approved = matches!(entry.decision, smista_core::api::ApprovalDecision::Approved);

        match kind {
            StorageApprovalKind::Plan => {
                let plan_id = plan_id_from_detail(detail)?;
                // An encrypted run deferred the user message and the plan
                // snapshot; write them now from the client's sealed map before
                // recording the decision against the plan.
                if !pending.is_empty() {
                    validate_required_seals(session, pending, &sealed).await?;
                    flush_pending_messages(session, user_id, session_id, pending, &sealed).await?;
                    write_pending_plan(session, user_id, session_id, pending, &sealed).await?;
                    reseal_run_input(session, &sealed).await?;
                }
                persist_approval(
                    session,
                    user_id,
                    session_id,
                    "plan",
                    &plan_id.to_string(),
                    entry.decision,
                    entry.reason,
                )
                .await?;
                let status = if approved {
                    smista_storage::entity::PlanStatus::Approved
                } else {
                    smista_storage::entity::PlanStatus::Rejected
                };
                session.set_plan_status(plan_id, status).await?;

                if !approved {
                    tracing::info!(%session_id, "plan rejected; returning the run to idle");
                    self.release(
                        session,
                        user_id,
                        session_id,
                        run_id,
                        cx.state.turn,
                        RunPhase::Idle,
                    )
                    .await?;
                    return Ok(idle_response());
                }
                tracing::info!(%session_id, "plan approved; leaving plan mode and continuing");
                self.clear_plan_active(session).await?;
                let step = self
                    .run_continuation_turn(
                        &resumed_cx,
                        ResumeStep::NextTurn,
                        Vec::new(),
                        &BTreeMap::new(),
                    )
                    .await?;
                self.finish_step(session, user_id, session_id, run_id, step)
                    .await
            }
            StorageApprovalKind::RemoteDisclosure | StorageApprovalKind::CostLimit => {
                persist_approval(
                    session,
                    user_id,
                    session_id,
                    "gate",
                    approval_id,
                    entry.decision,
                    entry.reason,
                )
                .await?;
                if !approved {
                    tracing::info!(%session_id, "gate rejected; returning the run to idle");
                    self.release(
                        session,
                        user_id,
                        session_id,
                        run_id,
                        cx.state.turn,
                        RunPhase::Idle,
                    )
                    .await?;
                    return Ok(idle_response());
                }
                let step = self
                    .run_continuation_turn(
                        &resumed_cx,
                        ResumeStep::Invoke,
                        Vec::new(),
                        &BTreeMap::new(),
                    )
                    .await?;
                self.finish_step(session, user_id, session_id, run_id, step)
                    .await
            }
        }
    }

    /// Clears the persisted plan-mode flag so the next turn is non-planning.
    async fn clear_plan_active(&self, session: &UserSession) -> Result<(), OrchestratorError> {
        let Some((mut input, content)) = session.run_input().await? else {
            return Err(OrchestratorError::Internal(
                "run input vanished before plan accept".to_string(),
            ));
        };
        input.plan_active = false;
        session.set_run_input(input, content).await?;
        Ok(())
    }

    /// Cancels every tool call a paused `AwaitingTool` phase is still waiting on.
    async fn cancel_outstanding_tools(
        &self,
        session: &UserSession,
        state: &RunState,
    ) -> Result<(), OrchestratorError> {
        let RunPhase::AwaitingTool { calls, .. } = &state.phase else {
            return Ok(());
        };
        for wait in calls {
            let Ok(call_uuid) = Uuid::parse_str(&wait.call_id) else {
                continue;
            };
            session
                .set_tool_call_outcome(
                    call_uuid,
                    ToolCallStatus::Failed,
                    None,
                    Some(SecretContent::plaintext("cancelled by supersede")),
                )
                .await?;
        }
        Ok(())
    }

    /// Builds the turn context for a continuation and runs one turn.
    async fn run_continuation_turn(
        &self,
        cx: &ContinuationCx<'_>,
        resume: ResumeStep,
        followups: Vec<RequestMessage>,
        decrypted: &BTreeMap<ContentRef, String>,
    ) -> Result<TurnStep, OrchestratorError> {
        // A stored bundle is re-read here, not taken from the snapshot `advance`
        // opened, so a reseal earlier in this same continuation (which overwrites
        // the placeholder with ciphertext) is visible to the turn.
        let fresh;
        let bundle = match cx.bundle {
            BundleSource::Loaded(loaded) => BundleSource::Loaded(loaded),
            BundleSource::Stored(_) => {
                let (_, content) = cx.session.run_input().await?.ok_or_else(|| {
                    OrchestratorError::Internal("run input is missing".to_string())
                })?;
                fresh = content;
                BundleSource::Stored(&fresh.content)
            }
        };
        let turn_cx = TurnCx {
            router: &self.router,
            resolver: &self.resolver,
            session: cx.session,
            credentials: cx.credentials,
            scope: MemoryScope {
                user_id: cx.session.user_id(),
                session_id: cx.session.session_id(),
            },
            cancel: cx.token,
            meta: cx.meta,
            bundle,
            // The bundle was sealed by the turn that authored the run; a
            // continuation never re-seals it.
            seal_run_input: false,
            plan_active: cx.plan_active,
            encrypted: is_encrypted(cx.session).await?,
            decrypted,
            sink: cx.sink,
        };
        Ok(run_turn(&turn_cx, resume, followups).await)
    }

    /// Maps a [`TurnStep`] onto a persisted checkpoint and a wire response.
    async fn finish_step(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        step: TurnStep,
    ) -> Result<TurnResponse, OrchestratorError> {
        match step {
            TurnStep::Completed(data) => {
                self.finish_completed(session, user_id, session_id, run_id, *data)
                    .await
            }
            TurnStep::AwaitingTool(data) => {
                self.pause_for_tools(session, user_id, session_id, run_id, *data)
                    .await
            }
            TurnStep::AwaitingApproval(data) => {
                self.pause_for_approval(session, user_id, session_id, run_id, *data)
                    .await
            }
            TurnStep::AwaitingDecrypt(references, to_encrypt) => {
                self.pause_for_decrypt(session, user_id, session_id, run_id, references, to_encrypt)
                    .await
            }
            TurnStep::Errored(error) => Err(error),
        }
    }

    /// Releases the lock on success, or rolls it back on failure.
    ///
    /// `restore` carries the checkpoint to fall back to when a continuation is
    /// rejected: a [preserving](OrchestratorError::preserves_checkpoint) error
    /// releases the lock back to that durable phase, leaving the run resumable;
    /// every other failure (and any `execute`, which passes `None`) rolls the
    /// run back to idle. The in-flight token is always dropped.
    async fn settle(
        &self,
        result: Result<TurnResponse, OrchestratorError>,
        session: &UserSession,
        run_id: Uuid,
        token: &TurnToken,
        restore: Option<(u32, RunPhase)>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let user_id = session.user_id();
        let session_id = session.session_id();
        match result {
            Ok(response) => {
                self.registry.finish(session_id, token);
                Ok(response)
            }
            Err(error) => {
                if let (true, Some((turn, phase))) = (error.preserves_checkpoint(), restore) {
                    tracing::warn!(%session_id, %run_id, %error, "continuation rejected; preserving the checkpoint");
                    if let Err(release_error) = self
                        .release(session, user_id, session_id, run_id, turn, phase)
                        .await
                    {
                        tracing::error!(%session_id, %release_error, "failed to release the lock after a rejected continuation");
                    }
                    self.registry.finish(session_id, token);
                } else {
                    tracing::warn!(%session_id, %run_id, %error, "turn failed; releasing the lock");
                    self.abort(session, user_id, session_id, run_id, token)
                        .await;
                }
                Err(error)
            }
        }
    }

    /// Marks the run's processing lock held while a continuation is served.
    async fn mark_active(
        &self,
        session: &UserSession,
        state: &RunState,
    ) -> Result<(), OrchestratorError> {
        let mut next = state.clone();
        next.active = Some(ActiveTurn {
            started_at: Utc::now(),
            lease: state.run_id.clone(),
        });
        session.set_run_state(next).await?;
        Ok(())
    }

    /// Atomically begins a fresh run, acquiring the lock.
    ///
    /// Generates a run id and writes a [`RunState`] with `active` set only when
    /// no turn holds the lock — the check and write are one transaction, so two
    /// concurrent `execute`s can never both start a run. Returns
    /// [`OrchestratorError::Busy`] when a turn is already in flight; otherwise
    /// begins the in-flight token and returns it.
    async fn acquire(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<(Uuid, TurnToken), OrchestratorError> {
        let run_id = Uuid::now_v7();
        let mut state = RunState::new(session_id, user_id, run_id, RunPhase::Idle);
        state.active = Some(ActiveTurn {
            started_at: Utc::now(),
            lease: run_id.to_string(),
        });
        if session.acquire_run_lock(state).await?.is_none() {
            tracing::warn!(%session_id, "rejecting execute: a turn is already in flight");
            return Err(OrchestratorError::Busy);
        }
        let token = self.registry.begin(session_id);
        Ok((run_id, token))
    }

    /// Writes the next durable phase with the lock released (`active = None`).
    async fn release(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        turn: u32,
        phase: RunPhase,
    ) -> Result<(), OrchestratorError> {
        let mut state = RunState::new(session_id, user_id, run_id, phase);
        state.turn = turn;
        state.active = None;
        session.set_run_state(state).await?;
        Ok(())
    }

    /// Best-effort lock release used when a turn fails before completing.
    async fn abort(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        token: &TurnToken,
    ) {
        if let Err(error) = self
            .release(session, user_id, session_id, run_id, 0, RunPhase::Idle)
            .await
        {
            tracing::error!(%session_id, %error, "failed to release the run lock after a failure");
        }
        self.registry.finish(session_id, token);
    }

    /// Persists a completed turn's work, releases the lock and builds the reply.
    ///
    /// The user and assistant messages were authored by the turn loop — stored
    /// directly for a plaintext session, deferred for an encrypted one. A
    /// plaintext turn is terminal: the run goes idle. An encrypted turn cannot
    /// store its messages until the client seals them, so the run parks at
    /// `AwaitingEncrypt`, returns the messages' plaintext in `to_encrypt`, and
    /// advertises `[sealed, break]`; the trailing `sealed` writes the rows.
    async fn finish_completed(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        data: CompletedData,
    ) -> Result<TurnResponse, OrchestratorError> {
        let CompletedData {
            resolved,
            content,
            usage,
            deferred,
        } = data;
        let provider = resolved.routing.provider.clone();
        let model = resolved.routing.model.clone();

        persist_routing_decision(session, user_id, session_id, &resolved.routing).await?;
        persist_context_references(session, user_id, session_id, &resolved.context.references)
            .await?;

        let encrypted = !deferred.pending.is_empty();
        let (allowed_continuations, to_encrypt) = if encrypted {
            // Session memory the model wrote during the run was stored in clear by
            // the memory tool; fold it into the same seal so nothing is left
            // readable at rest once the run ends.
            let mut to_encrypt = deferred.to_encrypt;
            for (id, plaintext) in clear_context_memory(session).await? {
                to_encrypt.insert(ContentRef::Memory(id.to_string()), plaintext);
            }
            let phase = RunPhase::AwaitingEncrypt {
                pending: deferred.pending,
                resume: ResumeStep::Finalize,
            };
            self.release(session, user_id, session_id, run_id, 1, phase)
                .await?;
            tracing::debug!(%session_id, %run_id, "turn completed; awaiting seal of authored content");
            (vec![ContinueKind::Sealed, ContinueKind::Break], to_encrypt)
        } else {
            self.release(session, user_id, session_id, run_id, 1, RunPhase::Idle)
                .await?;
            tracing::debug!(%session_id, %run_id, "turn completed; lock released, phase idle");
            (Vec::new(), BTreeMap::new())
        };

        let outcome = TurnOutcome::Completed(Box::new(CompletedTurn {
            message: Message {
                role: MessageRole::Assistant,
                content,
                provider: Some(provider),
                model: Some(model),
            },
            classification: resolved.classification,
            routing: routing_outcome(&resolved.routing),
            context: context_outcome(&resolved.context),
            usage,
            to_encrypt,
            trace_id: String::new(),
        }));
        Ok(TurnResponse {
            outcome,
            allowed_continuations,
        })
    }

    /// Checkpoints a turn paused on client-run tools and builds the reply.
    ///
    /// For a plaintext session the assistant tool-request message and tool-call
    /// rows were already written by the turn loop. For an encrypted session they
    /// were deferred: the phase carries their metadata as [`PendingWrite`]s and
    /// the response carries their plaintext in `to_encrypt`, to be sealed and
    /// written when the results arrive.
    async fn pause_for_tools(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        data: AwaitingToolData,
    ) -> Result<TurnResponse, OrchestratorError> {
        let AwaitingToolData {
            tool_requests,
            calls,
            deferred,
        } = data;

        let phase = RunPhase::AwaitingTool {
            calls,
            resume: ResumeStep::NextTurn,
            pending: deferred.pending,
        };
        self.release(session, user_id, session_id, run_id, 0, phase)
            .await?;
        tracing::debug!(%session_id, %run_id, "turn paused for tools; lock released");

        Ok(TurnResponse {
            outcome: TurnOutcome::AwaitingTool {
                tool_requests,
                to_encrypt: deferred.to_encrypt,
                trace_id: String::new(),
            },
            allowed_continuations: vec![
                ContinueKind::ToolResults,
                ContinueKind::Inject,
                ContinueKind::Break,
            ],
        })
    }

    /// Checkpoints a planning turn paused on a plan approval and builds the reply.
    ///
    /// For a plaintext session the plan snapshot was written by the turn loop; for
    /// an encrypted session it was deferred, so the phase carries the plan (and
    /// the user message) as [`PendingWrite`]s and the response carries their
    /// plaintext in `to_encrypt`, to be sealed and written with the decision.
    async fn pause_for_approval(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        data: AwaitingApprovalData,
    ) -> Result<TurnResponse, OrchestratorError> {
        let AwaitingApprovalData {
            approval_id,
            detail,
            deferred,
            ..
        } = data;
        let detail_json = serde_json::to_string(&detail).map_err(|error| {
            OrchestratorError::Internal(format!("approval detail encode: {error}"))
        })?;

        let phase = RunPhase::AwaitingApproval {
            approval_id: approval_id.clone(),
            kind: StorageApprovalKind::Plan,
            detail: detail_json,
            resume: ResumeStep::NextTurn,
            pending: deferred.pending,
        };
        self.release(session, user_id, session_id, run_id, 0, phase)
            .await?;
        tracing::debug!(%session_id, %run_id, "turn paused for plan approval; lock released");

        Ok(TurnResponse {
            outcome: TurnOutcome::AwaitingApproval {
                approval: PendingApproval {
                    approval_id,
                    kind: ApprovalKind::Plan,
                    detail,
                },
                to_encrypt: deferred.to_encrypt,
                trace_id: String::new(),
            },
            allowed_continuations: vec![ContinueKind::ApprovalDecisions, ContinueKind::Break],
        })
    }

    /// Checkpoints a turn paused to decrypt sealed history and builds the reply.
    ///
    /// Reaches here only for an encrypted session whose prior history is sealed:
    /// the orchestrator reads each sealed row into a `to_decrypt` map, parks the
    /// run at `AwaitingDecrypt` so the next `decrypted` continuation resumes prompt
    /// building, and advertises `[decrypted, break]`. When the authoring turn
    /// pauses here, `to_encrypt` carries its run-input bundle to seal in the same
    /// round, so the client opens history and seals the bundle together.
    async fn pause_for_decrypt(
        &self,
        session: &UserSession,
        user_id: Uuid,
        session_id: Uuid,
        run_id: Uuid,
        references: Vec<ContentRef>,
        to_encrypt: BTreeMap<ContentRef, String>,
    ) -> Result<TurnResponse, OrchestratorError> {
        let to_decrypt = crypto::build_to_decrypt(session, &references).await?;
        let records = references
            .iter()
            .filter_map(|reference| {
                crypto::content_ref_uuid(reference)
                    .ok()
                    .map(|id| RecordId::new(crypto::content_ref_table(reference), id.to_string()))
            })
            .collect();
        let phase = RunPhase::AwaitingDecrypt {
            records,
            resume: ResumeStep::BuildPrompt,
        };
        self.release(session, user_id, session_id, run_id, 0, phase)
            .await?;
        tracing::debug!(%session_id, %run_id, "turn paused to decrypt history; lock released");

        Ok(TurnResponse {
            outcome: TurnOutcome::AwaitingDecrypt {
                to_decrypt,
                to_encrypt,
                trace_id: String::new(),
            },
            allowed_continuations: vec![ContinueKind::Decrypted, ContinueKind::Break],
        })
    }
}

/// Whether the session is end-to-end encrypted.
async fn is_encrypted(session: &UserSession) -> Result<bool, OrchestratorError> {
    Ok(session
        .session()
        .await?
        .is_some_and(|metadata| metadata.encrypted))
}

/// Writes the deferred message rows of an encrypted pause, sealed by the client.
///
/// Each [`PendingWrite::Message`] is paired with the ciphertext the client
/// returned under its content reference and written as one row; other pending
/// kinds are left to their own writers.
async fn flush_pending_messages(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    pending: &[PendingWrite],
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> Result<(), OrchestratorError> {
    for write in pending {
        if let PendingWrite::Message {
            id,
            role,
            provider,
            model,
        } = write
        {
            let content = sealed_content(sealed, &ContentRef::Message(id.clone()))?;
            let uuid = Uuid::parse_str(id).map_err(|_| {
                OrchestratorError::Internal("pending message id is not a uuid".to_string())
            })?;
            write_sealed_message(
                session,
                user_id,
                session_id,
                uuid,
                *role,
                provider.clone(),
                model,
                content,
            )
            .await?;
        }
    }
    Ok(())
}

/// Writes the deferred plan row of an encrypted pause, if any, sealed by the client.
async fn write_pending_plan(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    pending: &[PendingWrite],
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> Result<(), OrchestratorError> {
    for write in pending {
        if let PendingWrite::Plan { id } = write {
            let content = sealed_content(sealed, &ContentRef::Plan(id.clone()))?;
            let uuid = Uuid::parse_str(id).map_err(|_| {
                OrchestratorError::Internal("pending plan id is not a uuid".to_string())
            })?;
            write_sealed_plan(session, user_id, session_id, uuid, content).await?;
        }
    }
    Ok(())
}

/// The session-memory rows still stored in clear, as `(id, plaintext)`.
///
/// The memory tool writes session memory in clear; an encrypted run seals these
/// rows before it ends so nothing readable is left at rest. User memory is
/// user-scoped and out of the run's scope, so it is not touched here.
async fn clear_context_memory(
    session: &UserSession,
) -> Result<Vec<(Uuid, String)>, OrchestratorError> {
    let rows = session.list_context_memory_with_content().await?;
    Ok(rows
        .into_iter()
        .filter_map(|(memory, content)| {
            content
                .content
                .as_plaintext()
                .map(|plaintext| (memory.uuid(), plaintext.to_string()))
        })
        .collect())
}

/// Seals the session-memory rows the client returned, overwriting their content.
///
/// Unlike a deferred message, a memory row already exists (the tool wrote it in
/// clear), so the ciphertext is written in place with [`UserSession::set_content`].
async fn reseal_memory(
    session: &UserSession,
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> Result<(), OrchestratorError> {
    for (reference, payload) in sealed {
        if let ContentRef::Memory(id) = reference {
            let uuid = Uuid::parse_str(id).map_err(|_| {
                OrchestratorError::Internal("sealed memory id is not a uuid".to_string())
            })?;
            session
                .set_content(
                    "context_memory",
                    uuid,
                    SecretContent::Encrypted(crypto::payload_to_envelope(payload)),
                )
                .await?;
        }
    }
    Ok(())
}

/// Seals the run-input bundle the client returned, overwriting its placeholder.
///
/// The run-input content row already exists (the run wrote a placeholder up
/// front), so the ciphertext is written in place. A no-op when the seal map
/// carries no run-input entry — a plaintext run never defers it.
async fn reseal_run_input(
    session: &UserSession,
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> Result<(), OrchestratorError> {
    let reference = ContentRef::RunInput(session.session_id().to_string());
    if let Some(payload) = sealed.get(&reference) {
        session
            .set_content(
                "session_run_input",
                session.session_id(),
                SecretContent::Encrypted(crypto::payload_to_envelope(payload)),
            )
            .await?;
    }
    Ok(())
}

/// The content reference a deferred [`PendingWrite`] expects the client to seal.
fn pending_content_ref(write: &PendingWrite) -> ContentRef {
    match write {
        PendingWrite::Message { id, .. } => ContentRef::Message(id.clone()),
        PendingWrite::ToolCall { id, .. } => ContentRef::ToolCall(id.clone()),
        PendingWrite::Plan { id } => ContentRef::Plan(id.clone()),
    }
}

/// Whether the run-input bundle is still its unsealed placeholder.
///
/// An encrypted run writes an empty plaintext placeholder up front and seals it
/// on the first answering continuation; once sealed its content is an envelope,
/// and a plaintext run stores the real bundle. Only the empty placeholder means
/// this continuation must carry the run-input ciphertext.
async fn run_input_unsealed(session: &UserSession) -> Result<bool, OrchestratorError> {
    let Some((_, content)) = session.run_input().await? else {
        return Ok(false);
    };
    Ok(content.content.as_plaintext() == Some(""))
}

/// Validates the client's sealed map carries every ciphertext this pause needs.
///
/// An encrypted continuation seals the rows the pause deferred (`pending`) plus,
/// on the first answering continuation, the run-input bundle. A map that omits
/// any of these would write some rows and silently skip others — `reseal_run_input`
/// no-ops on a missing bundle — leaving a half-written checkpoint and an
/// unparseable placeholder for the next turn. Rejecting the continuation before
/// any write keeps the checkpoint answerable.
async fn validate_required_seals(
    session: &UserSession,
    pending: &[PendingWrite],
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
) -> Result<(), OrchestratorError> {
    let mut required: Vec<ContentRef> = pending.iter().map(pending_content_ref).collect();
    if run_input_unsealed(session).await? {
        required.push(ContentRef::RunInput(session.session_id().to_string()));
    }
    if let Some(missing) = required
        .iter()
        .find(|reference| !sealed.contains_key(reference))
    {
        tracing::warn!(
            session_id = %session.session_id(),
            reference = %missing.id(),
            "rejecting continuation: a required sealed payload is missing"
        );
        return Err(OrchestratorError::UnexpectedContinuation);
    }
    Ok(())
}

/// The tool name of the pending tool-call row matching `call_id`, if recorded.
fn pending_tool_name(pending: &[PendingWrite], call_id: &str) -> Option<String> {
    pending.iter().find_map(|write| match write {
        PendingWrite::ToolCall { id, tool_name } if id == call_id => Some(tool_name.clone()),
        _ => None,
    })
}

/// The client-sealed content for `reference`, or an error when it is missing.
fn sealed_content(
    sealed: &BTreeMap<ContentRef, EncryptedPayload>,
    reference: &ContentRef,
) -> Result<SecretContent, OrchestratorError> {
    let payload = sealed.get(reference).ok_or_else(|| {
        OrchestratorError::Internal(format!("missing ciphertext for {}", reference.id()))
    })?;
    Ok(SecretContent::Encrypted(crypto::payload_to_envelope(
        payload,
    )))
}

/// Builds the `idle` acknowledgement returned when a run finishes with nothing
/// to render.
fn idle_response() -> TurnResponse {
    TurnResponse {
        outcome: TurnOutcome::Idle {
            trace_id: String::new(),
        },
        allowed_continuations: Vec::new(),
    }
}

/// Extracts the drafted plan's id from a persisted approval `detail` payload.
fn plan_id_from_detail(detail: &str) -> Result<Uuid, OrchestratorError> {
    let value: serde_json::Value = serde_json::from_str(detail)
        .map_err(|error| OrchestratorError::Internal(format!("approval detail decode: {error}")))?;
    value
        .get("plan")
        .and_then(serde_json::Value::as_str)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| OrchestratorError::Internal("approval detail has no plan id".to_string()))
}

/// Names the [`ContinueKind`] of a continuation, for admission.
fn continue_kind(continuation: &ContinueRequest) -> ContinueKind {
    match continuation {
        ContinueRequest::ToolResults { .. } => ContinueKind::ToolResults,
        ContinueRequest::ApprovalDecisions { .. } => ContinueKind::ApprovalDecisions,
        ContinueRequest::Decrypted { .. } => ContinueKind::Decrypted,
        ContinueRequest::Sealed { .. } => ContinueKind::Sealed,
        ContinueRequest::Inject { .. } => ContinueKind::Inject,
        ContinueRequest::Break => ContinueKind::Break,
    }
}

/// Maps the resolver's [`RoutingDecision`] onto the wire [`RoutingOutcome`].
fn routing_outcome(decision: &RoutingDecision) -> RoutingOutcome {
    RoutingOutcome {
        task_type: decision.intent,
        provider: decision.provider.clone(),
        model: decision.model.clone(),
        matched_rule: decision.matched_rule.clone(),
        fallback_used: decision.fallback_used,
        override_used: decision.override_used,
    }
}

/// Maps the resolver's finalized context onto the wire [`ContextOutcome`].
fn context_outcome(context: &ResolvedContext) -> ContextOutcome {
    ContextOutcome {
        included: context.outcome.included.clone(),
        excluded: context.outcome.excluded.clone(),
    }
}
