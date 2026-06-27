//! The per-turn loop body: recall → resolve → prompt → invoke → branch.
//!
//! [`run_turn`] runs the deterministic pipeline for one turn and reports what
//! happened as a [`TurnStep`] the orchestrator maps onto a durable phase and a
//! wire response. The loop reuses the pure [`Resolver`] verbatim: it only feeds
//! it the recalled, plaintext inputs and acts on its output, so routing never
//! depends on an LLM.
#![allow(
    dead_code,
    reason = "the turn loop grows tool, approval and decrypt branches in later tasks"
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use secrecy::SecretString;
use smista_core::api::{ContentRef, ToolApproval, ToolRequest};
use smista_core::message::MessageRole;
use smista_core::model::{Provider, RoutingRequirements};
use smista_core::usage::Usage;
use smista_providers::api::{CompletionRequest, RequestMessage, ToolChoice};
use smista_providers::memory::MemoryScope;
use smista_storage::entity::{
    PendingWrite, ResumeStep, ToolApproval as StorageToolApproval, ToolWait,
};
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::orchestrator::cost::priced;
use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::invoke::invoke;
use crate::orchestrator::mediation::mediate;
use crate::orchestrator::persist::{
    persist_plaintext_message, persist_plan_draft, persist_tool_request,
};
use crate::orchestrator::prompt::build_messages;
use crate::orchestrator::recall::{Recalled, recall};
use crate::orchestrator::registry::TurnToken;
use crate::orchestrator::run_input::{RunInputBundle, RunInputMeta, rebuild_workspace};
use crate::orchestrator::tools::offered_tools;
use crate::router::Router;
use crate::router::resolver::{ResolveArgs, ResolvedTurn, Resolver};
use crate::session::UserSession;

/// The base system prompt every turn frames its prompt with.
const PREAMBLE: &str = "\
You are a capable assistant. Complete the task you are given accurately and \
concisely, using the tools available to you.";

/// The timeout applied to each provider when listing the model catalog.
pub(super) const CATALOG_TIMEOUT: Duration = Duration::from_secs(30);

/// The borrowed inputs the turn loop needs for one turn.
pub(crate) struct TurnCx<'a> {
    /// The provider registry the chosen model is resolved through.
    pub(crate) router: &'a Router,
    /// The deterministic, LLM-free routing pipeline.
    pub(crate) resolver: &'a Resolver,
    /// The session whose history and memory the turn recalls.
    pub(crate) session: &'a UserSession,
    /// Provider credentials supplied with the request, used for this turn only.
    pub(crate) credentials: &'a HashMap<Provider, SecretString>,
    /// The memory scope (user + session) memory operations are confined to.
    pub(crate) scope: MemoryScope,
    /// The cancellation handle a superseding request trips.
    pub(crate) cancel: &'a TurnToken,
    /// The run's non-secret request context (policy, preferences, workspace).
    pub(crate) meta: &'a RunInputMeta,
    /// The run's sealable request context (input, attachments, git diff), either
    /// in memory for a fresh `execute` or recalled — possibly sealed — for a
    /// continuation.
    pub(crate) bundle: BundleSource<'a>,
    /// Whether this turn must seal the run-input bundle: set only by the fresh
    /// `execute` that authored it, so the bundle is folded into the seal once and
    /// not re-sealed on later turns.
    pub(crate) seal_run_input: bool,
    /// Whether the run is in plan mode: file-changing tools are denied and a
    /// completing turn snapshots a plan for approval instead of answering.
    pub(crate) plan_active: bool,
    /// Whether the session is end-to-end encrypted. When set the turn stores no
    /// router-authored content; it folds the plaintext into `to_encrypt` and
    /// carries each row's metadata as a [`PendingWrite`] to be written, paired
    /// with the client's ciphertext, on the answering continuation.
    pub(crate) encrypted: bool,
    /// History the client has already opened, keyed by [`ContentRef`]. Empty for
    /// a plaintext session; on an encrypted session it carries the plaintext that
    /// lets recall build the prompt without re-pausing for decryption.
    pub(crate) decrypted: &'a BTreeMap<ContentRef, String>,
    /// The live event sink for a streamed turn. `None` for a buffered turn; when
    /// set, the invoke step forwards text and reasoning deltas as they arrive.
    pub(crate) sink: Option<&'a crate::orchestrator::stream::TurnSink>,
}

/// Where a turn's run-input bundle comes from.
///
/// A fresh `execute` carries the bundle in memory ([`Loaded`](Self::Loaded)); a
/// continuation recalls it from its stored content row ([`Stored`](Self::Stored)),
/// which is plaintext for an unencrypted session and sealed otherwise. A sealed
/// bundle is opened through the same decrypt handshake as history.
#[derive(Clone, Copy)]
pub(crate) enum BundleSource<'a> {
    /// The bundle is already in memory; use it directly.
    Loaded(&'a RunInputBundle),
    /// The bundle lives in a stored content row, possibly sealed.
    Stored(&'a SecretContent),
}

/// Router-authored content an encrypted turn defers: what the client must seal
/// (`to_encrypt`) and the row metadata to write once it returns (`pending`).
///
/// Both are empty for a plaintext turn, which stores its content directly.
#[derive(Default)]
pub(crate) struct Deferred {
    /// Plaintext the client seals, keyed by the row's content reference.
    pub(crate) to_encrypt: BTreeMap<ContentRef, String>,
    /// Metadata of the rows written once the ciphertext returns.
    pub(crate) pending: Vec<PendingWrite>,
}

/// What a completed turn carries back for the orchestrator to persist and serve.
pub(crate) struct CompletedData {
    /// The deterministic resolution that drove the turn.
    pub(crate) resolved: ResolvedTurn,
    /// The assistant's reply text.
    pub(crate) content: String,
    /// Token usage reported by the provider, before pricing.
    pub(crate) usage: Usage,
    /// The deferred content of an encrypted turn (the user and assistant
    /// messages); empty for a plaintext turn, which stored them directly.
    pub(crate) deferred: Deferred,
}

/// What a tool-request pause carries back for the orchestrator to checkpoint.
pub(crate) struct AwaitingToolData {
    /// The calls the client must run, correlated by `call_id`.
    pub(crate) tool_requests: Vec<ToolRequest>,
    /// The outstanding waits recorded in the durable `AwaitingTool` phase.
    pub(crate) calls: Vec<ToolWait>,
    /// The deferred content of an encrypted turn (the user and assistant
    /// messages and the tool-call rows); empty for a plaintext turn.
    pub(crate) deferred: Deferred,
}

/// What a plan-approval pause carries back for the orchestrator to checkpoint.
pub(crate) struct AwaitingApprovalData {
    /// Identifier the client echoes back with its decision.
    pub(crate) approval_id: String,
    /// The drafted plan the decision applies to.
    pub(crate) plan_id: uuid::Uuid,
    /// Non-secret detail re-emitted to the client (the plan reference).
    pub(crate) detail: serde_json::Value,
    /// The deferred content of an encrypted turn (the user message and the plan
    /// snapshot); empty for a plaintext turn.
    pub(crate) deferred: Deferred,
}

/// The outcome of one turn the orchestrator maps onto a phase and a response.
pub(crate) enum TurnStep {
    /// The model produced a final answer with no outstanding tool calls.
    Completed(Box<CompletedData>),
    /// The model requested one or more client-run tools; the run pauses.
    AwaitingTool(Box<AwaitingToolData>),
    /// A planning turn finished; the run pauses on a plan approval.
    AwaitingApproval(Box<AwaitingApprovalData>),
    /// Recall needs sealed history opened before the prompt can be built; the run
    /// pauses on a decrypt request for the listed rows, carrying any run-input
    /// bundle the authoring turn must seal in the same round.
    AwaitingDecrypt(Vec<ContentRef>, BTreeMap<ContentRef, String>),
    /// The turn could not proceed; the orchestrator releases the lock and
    /// surfaces the error.
    Errored(OrchestratorError),
}

/// Runs one turn, reporting the outcome as a [`TurnStep`].
///
/// `resume` selects where a resumed run re-enters the loop; a fresh run starts
/// from the beginning. `followups` seeds the conversation with messages a
/// continuation already produced — the tool results that answer an
/// `awaiting_tool` pause — so the model sees them on the next turn.
pub(crate) async fn run_turn(
    cx: &TurnCx<'_>,
    resume: ResumeStep,
    followups: Vec<RequestMessage>,
) -> TurnStep {
    match run_turn_inner(cx, resume, followups).await {
        Ok(step) => step,
        Err(error) => TurnStep::Errored(error),
    }
}

/// The fallible body of [`run_turn`]; any error becomes [`TurnStep::Errored`].
///
/// The loop drives one turn to a checkpoint. A completion with no tool calls
/// ends the turn; tool calls are mediated against the deterministic tool policy.
/// Denied-only calls are refused and fed back to the model, which re-generates
/// within the same turn; a call the client must run pauses the turn at
/// `awaiting_tool`. Plan mode is not engaged on this path.
async fn run_turn_inner(
    cx: &TurnCx<'_>,
    resume: ResumeStep,
    followups: Vec<RequestMessage>,
) -> Result<TurnStep, OrchestratorError> {
    let session_id = cx.session.session_id();
    // Resolve the bundle once for this turn. A loaded bundle is ready; a stored
    // one is parsed from its plaintext or, when sealed, from the client-opened
    // `decrypted` map — and is otherwise the run-input row the turn must pause to
    // decrypt alongside history.
    let (bundle, bundle_needs_decrypt) = match cx.bundle {
        BundleSource::Loaded(loaded) => (Some(loaded.clone()), false),
        BundleSource::Stored(content) => match resolve_bundle(content, session_id, cx.decrypted)? {
            Some(parsed) => (Some(parsed), false),
            None => (None, true),
        },
    };

    let input = bundle
        .as_ref()
        .map(|bundle| to_resolver_input(&bundle.input));
    // The tool results a continuation already produced, plus any denials this
    // turn feeds back so the model can choose another tool.
    let mut tool_followups: Vec<RequestMessage> = followups;
    // The user message is recorded once, on a fresh run's first turn, so it
    // precedes every assistant message the run authors.
    let mut user_recorded = !matches!(resume, ResumeStep::BuildPrompt);
    // Router-authored content this turn defers when the session is encrypted.
    let mut deferred = Deferred::default();

    // On the turn that authored the run, fold its bundle into the seal up front —
    // before recall — so the prompt, attachments and diff are carried out even
    // when the turn pauses to decrypt history, and never left readable at rest.
    if cx.seal_run_input
        && cx.encrypted
        && let Some(bundle) = bundle.as_ref()
    {
        let bundle_json = serde_json::to_string(bundle).map_err(|error| {
            OrchestratorError::Internal(format!("run-input bundle encode: {error}"))
        })?;
        deferred
            .to_encrypt
            .insert(ContentRef::RunInput(session_id.to_string()), bundle_json);
    }

    loop {
        // Recall the prior context, and fold in the run-input bundle when it too
        // is still sealed, so one decrypt request opens history and bundle
        // together. A plaintext session is always Ready. A decrypt pause carries
        // the deferred bundle so the client seals it alongside opening history.
        let recalled = match recall(cx.session, cx.decrypted).await? {
            Recalled::Ready(recalled) => {
                if bundle_needs_decrypt {
                    let reference = ContentRef::RunInput(session_id.to_string());
                    tracing::debug!("pausing the turn to decrypt the run-input bundle");
                    return Ok(TurnStep::AwaitingDecrypt(
                        vec![reference],
                        deferred.to_encrypt,
                    ));
                }
                recalled
            }
            Recalled::NeedsDecrypt(mut references) => {
                if bundle_needs_decrypt {
                    references.push(ContentRef::RunInput(session_id.to_string()));
                }
                tracing::debug!(
                    rows = references.len(),
                    "pausing the turn to decrypt history"
                );
                return Ok(TurnStep::AwaitingDecrypt(references, deferred.to_encrypt));
            }
        };

        // Past the decrypt gate the bundle is always resolved.
        let bundle = bundle
            .as_ref()
            .expect("bundle resolved past the decrypt gate");
        let input = input
            .as_ref()
            .expect("input resolved past the decrypt gate");

        // Resolve the route deterministically over the recalled inputs. The wire
        // request types are mapped into the resolver's own input forms.
        let catalog = cx
            .router
            .fetch_models(cx.credentials.clone(), CATALOG_TIMEOUT)
            .await
            .models;
        let credentialed: HashSet<Provider> = cx.credentials.keys().cloned().collect();
        let wire_workspace = rebuild_workspace(cx.meta, bundle);
        let workspace = to_resolver_workspace(&wire_workspace);
        let attachments = to_resolver_attachments(&bundle.attachments);
        let resolved = cx.resolver.resolve(ResolveArgs {
            input,
            workspace: &workspace,
            attachments: &attachments,
            recalled: &recalled,
            classification: &cx.meta.policy.classification,
            routing: &cx.meta.policy.routing,
            privacy: &cx.meta.policy.privacy,
            requirements: RoutingRequirements::default(),
            catalog: &catalog,
            credentialed: &credentialed,
            local_only: cx.meta.local_preferences.local_only,
        })?;

        // Record the user message once, stamped with the route serving the run,
        // so it lands in history ahead of any assistant message this run writes.
        // An encrypted run stores nothing now: it defers the row instead.
        if !user_recorded {
            author_message(
                cx,
                &mut deferred,
                MessageRole::User,
                &resolved.routing.provider,
                &resolved.routing.model,
                &bundle.input.text,
            )
            .await?;
            user_recorded = true;
        }

        // Build the prompt, offer the catalog of tools, and invoke the model.
        let messages = build_messages(
            PREAMBLE,
            &resolved.context,
            &recalled.messages,
            input,
            &tool_followups,
        );
        let tools = offered_tools(
            &cx.meta.policy.tools,
            &bundle.attachments.invoked_skills,
            &bundle.attachments.available_skills,
        );
        let tool_choice = if tools.is_empty() {
            ToolChoice::None
        } else {
            ToolChoice::Auto
        };
        let request = CompletionRequest {
            messages,
            parameters: resolved.model.default_parameters.clone(),
            tools,
            tool_choice,
        };
        let response = invoke(
            cx.router,
            &resolved,
            cx.credentials,
            cx.scope,
            request,
            cx.cancel.cancellation(),
            cx.sink,
        )
        .await?;

        if response.tool_calls.is_empty() {
            // A planning turn does not answer the user: it snapshots a plan and
            // pauses for approval. Any other turn returns the model's reply.
            if cx.plan_active {
                let plan_id = author_plan(cx, &mut deferred, &response.content).await?;
                tracing::debug!(%plan_id, "planning turn complete; pausing for plan approval");
                let approval_id = Uuid::now_v7().to_string();
                return Ok(TurnStep::AwaitingApproval(Box::new(AwaitingApprovalData {
                    approval_id,
                    plan_id,
                    detail: serde_json::json!({ "plan": plan_id.to_string() }),
                    deferred,
                })));
            }
            let usage = priced(response.usage, &resolved.model);
            author_message(
                cx,
                &mut deferred,
                MessageRole::Assistant,
                &resolved.routing.provider,
                &resolved.routing.model,
                &response.content,
            )
            .await?;
            return Ok(TurnStep::Completed(Box::new(CompletedData {
                resolved,
                content: response.content,
                usage,
                deferred,
            })));
        }

        // Partition the requested calls against the tool policy. In plan mode,
        // file-changing tools are refused.
        let mediated = mediate(
            response.tool_calls.clone(),
            &cx.meta.policy.tools,
            cx.plan_active,
        );

        if mediated.client.is_empty() {
            // Every call was refused: feed the denials back and let the model
            // try again within this same turn instead of pausing. Nothing is
            // recorded for the refused round.
            tracing::debug!(
                denied = mediated.denied.len(),
                "every requested tool was denied; feeding the refusals back to the model"
            );
            tool_followups.push(RequestMessage::Assistant {
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
            });
            for (call, reason) in &mediated.denied {
                tool_followups.push(RequestMessage::ToolResult {
                    call_id: call.call_id.clone(),
                    content: (*reason).to_string(),
                    is_error: true,
                });
            }
            continue;
        }

        // Record the assistant tool-request message and a row per client-bound
        // call, then pause. An encrypted run defers both instead of storing them.
        tracing::debug!(
            calls = mediated.client.len(),
            "pausing the turn for client-run tools"
        );
        let (tool_requests, calls) = author_tool_request(
            cx,
            &mut deferred,
            &resolved.routing.provider,
            &resolved.routing.model,
            &response.content,
            &mediated.client,
        )
        .await?;
        return Ok(TurnStep::AwaitingTool(Box::new(AwaitingToolData {
            tool_requests,
            calls,
            deferred,
        })));
    }
}

/// Resolves a stored run-input bundle to plaintext, or reports it needs decrypt.
///
/// Returns the parsed bundle when the content row is stored in clear or its
/// opened plaintext is in `decrypted`; returns `None` when the row is sealed and
/// not yet opened, so the turn pauses to decrypt it alongside history.
fn resolve_bundle(
    content: &SecretContent,
    session_id: Uuid,
    decrypted: &BTreeMap<ContentRef, String>,
) -> Result<Option<RunInputBundle>, OrchestratorError> {
    let json = if let Some(plaintext) = content.as_plaintext() {
        plaintext.to_string()
    } else {
        match decrypted.get(&ContentRef::RunInput(session_id.to_string())) {
            Some(opened) => opened.clone(),
            None => return Ok(None),
        }
    };
    let bundle = serde_json::from_str(&json).map_err(|error| {
        OrchestratorError::Internal(format!("run-input bundle decode: {error}"))
    })?;
    Ok(Some(bundle))
}

/// Records one message, or defers it when the session is encrypted.
///
/// A plaintext session stores the message directly. An encrypted session stores
/// nothing: it folds the text into `to_encrypt` for the client to seal and
/// records the row's metadata as a [`PendingWrite`] so the orchestrator can write
/// it once the ciphertext returns. Returns the message's id.
async fn author_message(
    cx: &TurnCx<'_>,
    deferred: &mut Deferred,
    role: MessageRole,
    provider: &Provider,
    model: &str,
    text: &str,
) -> Result<Uuid, OrchestratorError> {
    if cx.encrypted {
        let id = Uuid::now_v7();
        deferred.pending.push(PendingWrite::Message {
            id: id.to_string(),
            role,
            provider: provider.clone(),
            model: model.to_string(),
        });
        deferred
            .to_encrypt
            .insert(ContentRef::Message(id.to_string()), text.to_string());
        Ok(id)
    } else {
        Ok(persist_plaintext_message(
            cx.session,
            cx.session.user_id(),
            cx.session.session_id(),
            role,
            provider.clone(),
            model,
            text,
        )
        .await?)
    }
}

/// Snapshots the drafted plan, or defers it when the session is encrypted.
async fn author_plan(
    cx: &TurnCx<'_>,
    deferred: &mut Deferred,
    body: &str,
) -> Result<Uuid, OrchestratorError> {
    if cx.encrypted {
        let id = Uuid::now_v7();
        deferred
            .pending
            .push(PendingWrite::Plan { id: id.to_string() });
        deferred
            .to_encrypt
            .insert(ContentRef::Plan(id.to_string()), body.to_string());
        Ok(id)
    } else {
        Ok(persist_plan_draft(
            cx.session,
            cx.session.user_id(),
            cx.session.session_id(),
            body,
        )
        .await?)
    }
}

/// Records the assistant tool-request message and the client-bound call rows, or
/// defers them when the session is encrypted, returning the client's view.
///
/// The arguments handed to the client are never sealed under the tool call's own
/// reference; the request rides the sealed assistant message and the durable tool
/// secret is the `result`, sealed by the client on the continuation.
async fn author_tool_request(
    cx: &TurnCx<'_>,
    deferred: &mut Deferred,
    provider: &Provider,
    model: &str,
    assistant_text: &str,
    client: &[crate::orchestrator::mediation::ClientCall],
) -> Result<(Vec<ToolRequest>, Vec<ToolWait>), OrchestratorError> {
    if cx.encrypted {
        author_message(
            cx,
            deferred,
            MessageRole::Assistant,
            provider,
            model,
            assistant_text,
        )
        .await?;
        let mut tool_requests = Vec::with_capacity(client.len());
        let mut calls = Vec::with_capacity(client.len());
        for client_call in client {
            let call_id = Uuid::now_v7().to_string();
            deferred.pending.push(PendingWrite::ToolCall {
                id: call_id.clone(),
                tool_name: client_call.call.name.clone(),
            });
            tool_requests.push(ToolRequest {
                call_id: call_id.clone(),
                name: client_call.call.name.clone(),
                arguments: client_call.call.arguments.clone(),
                requires_approval: client_call.requires_approval,
            });
            calls.push(ToolWait {
                call_id,
                requires_approval: to_storage_approval(client_call.requires_approval),
            });
        }
        Ok((tool_requests, calls))
    } else {
        let persisted = persist_tool_request(
            cx.session,
            cx.session.user_id(),
            cx.session.session_id(),
            provider.clone(),
            model,
            assistant_text,
            client,
        )
        .await?;
        let tool_requests = persisted
            .calls
            .iter()
            .map(|call| ToolRequest {
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                requires_approval: call.requires_approval,
            })
            .collect();
        let calls = persisted
            .calls
            .iter()
            .map(|call| ToolWait {
                call_id: call.call_id.clone(),
                requires_approval: to_storage_approval(call.requires_approval),
            })
            .collect();
        Ok((tool_requests, calls))
    }
}

/// Maps the wire [`ToolApproval`] onto the storage [`StorageToolApproval`].
fn to_storage_approval(approval: ToolApproval) -> StorageToolApproval {
    match approval {
        ToolApproval::Allow => StorageToolApproval::Allow,
        ToolApproval::Ask => StorageToolApproval::Ask,
    }
}

/// Maps a wire [`TaskInput`](smista_core::api::TaskInput) into the resolver's.
pub(super) fn to_resolver_input(
    input: &smista_core::api::TaskInput,
) -> crate::router::resolver::TaskInput {
    crate::router::resolver::TaskInput {
        text: input.text.clone(),
        command: input.command,
        explicit_model: input.explicit_model.clone(),
    }
}

/// Maps a wire [`Workspace`](smista_core::api::Workspace) into the resolver's.
pub(super) fn to_resolver_workspace(
    workspace: &smista_core::api::Workspace,
) -> crate::router::resolver::Workspace {
    crate::router::resolver::Workspace {
        root: workspace.root.clone(),
        git_branch: workspace.git_branch.clone(),
        git_diff: workspace.git_diff.clone(),
        referenced_paths: workspace.referenced_paths.clone(),
        active_file: workspace.active_file.clone(),
    }
}

/// Maps wire [`Attachments`](smista_core::api::Attachments) into the resolver's.
pub(super) fn to_resolver_attachments(
    attachments: &smista_core::api::Attachments,
) -> crate::router::resolver::Attachments {
    crate::router::resolver::Attachments {
        files: attachments
            .files
            .iter()
            .map(|file| crate::router::resolver::ContextFile {
                path: file.path.clone(),
                content: file.content.clone(),
                required: file.required,
            })
            .collect(),
        instructions: attachments
            .instructions
            .iter()
            .map(|instruction| crate::router::resolver::ContextInstruction {
                source: instruction.source.clone(),
                content: instruction.content.clone(),
            })
            .collect(),
        invoked_skills: attachments.invoked_skills.clone(),
        available_skills: attachments.available_skills.clone(),
    }
}
