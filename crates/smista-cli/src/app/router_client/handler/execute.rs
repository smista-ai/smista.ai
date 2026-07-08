//! Execute command handler.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;
use sha2::{Digest, Sha256};
use smista_sdk::client::Client as _;
use smista_sdk::core::api::{
    Attachments, ContextFile, ContextInstruction, ExecutePolicy, ExecuteRequest, LocalPreferences,
    TaskInput, Workspace,
};
use smista_sdk::core::intent::TaskIntent;
use smista_sdk::core::model::ModelReference;
use smista_sdk::core::skill::Skill;

use crate::app::router_client::state::State;
use crate::app::router_client::{Msg, RouterClient};
use crate::skills::SkillStore;

const AGENTS_MD: &str = "AGENTS.md";

impl RouterClient {
    /// Handles the execution of an `execute` command.
    ///
    /// On success, sends a [`Msg::StreamedContentChunk`] to the UI for each chunk of streamed content.
    /// On failure, sends a [`Msg::Error`] to the UI with the error message.
    pub(in crate::app::router_client) async fn execute(
        &mut self,
        prompt: String,
        files: HashSet<PathBuf>,
        plan: bool,
        explicit_model: Option<ModelReference>,
    ) {
        tracing::debug!(
            explicit_model = explicit_model.as_ref().map(ToString::to_string),
            files = ?files.iter().collect::<Vec<_>>(),
            plan,
            prompt.bytes = prompt.len(),
            "executing prompt",
        );

        let session_id = match self.session_id_or_new(&prompt).await {
            Ok(session_id) => session_id,
            Err(err) => {
                tracing::error!("failed to initialize session: {err}");
                self.send_msg(Msg::Error(format!(
                    "Failed to initialize a new session: {err}"
                )))
                .await;
                return;
            }
        };

        let execute_request = self
            .build_execute_request(prompt, files, plan, explicit_model)
            .await;

        // send Thinking state
        self.send_msg(Msg::Thinking).await;

        match self
            .context
            .router_client
            .stream_execute(session_id, execute_request)
            .await
        {
            Ok(response) => {
                tracing::debug!("execute stream accepted, starting to stream results");

                self.state = State::Streaming;
                self.handle_turn_stream(response).await;
                if self.state == State::Streaming {
                    self.state = State::Idle;
                }
            }
            Err(err) => {
                tracing::error!("failed to execute prompt: {err}");
                self.state = State::Idle;
                self.send_msg(Msg::Error(format!(
                    "Failed to execute prompt through router: {err}"
                )))
                .await;
            }
        };

        if self.state == State::Idle {
            self.send_msg(Msg::Idle).await;
        }
    }

    /// Build a [`ExecuteRequest`] from the given prompt, files, and plan flag.
    pub(in crate::app::router_client) async fn build_execute_request(
        &self,
        prompt: String,
        files: HashSet<PathBuf>,
        plan: bool,
        explicit_model: Option<ModelReference>,
    ) -> ExecuteRequest {
        let command = if plan {
            tracing::debug!("planning enabled, generating plan command");
            Some(TaskIntent::Plan)
        } else {
            None
        };
        let mut referenced_paths = files.into_iter().collect::<Vec<_>>();
        referenced_paths.sort();
        let (git_branch, git_diff) = git_snapshot(&self.context.cwd);
        let attached_files = load_context_files(&self.context.cwd, &referenced_paths).await;
        let instructions = load_instructions(&self.context.cwd).await;
        let available_skills = load_available_skills(&self.context.skills_store);

        ExecuteRequest {
            input: TaskInput {
                text: prompt,
                command,
                explicit_model,
            },
            workspace: Workspace {
                root: self.context.cwd.clone(),
                git_branch,
                git_diff,
                referenced_paths,
                active_file: None,
            },
            policy: ExecutePolicy::v1(
                "merged",
                self.context.config.classification.clone(),
                self.context.config.routing.clone(),
                self.context.config.tools.clone(),
                self.context.config.privacy.clone(),
            ),
            local_preferences: LocalPreferences {
                auto_apply: self.context.config.local.auto_apply.unwrap_or_default(),
                local_only: self.context.config.local.local_only.unwrap_or_default(),
                no_network: self.context.config.local.no_network.unwrap_or_default(),
            },
            attachments: Attachments {
                files: attached_files,
                instructions,
                invoked_skills: Vec::new(),
                available_skills,
            },
        }
    }

    /// Returns the active session identifier or creates a new session for `prompt`.
    pub(in crate::app::router_client) async fn session_id_or_new(
        &mut self,
        prompt: &str,
    ) -> anyhow::Result<uuid::Uuid> {
        if let Some(session_id) = self.session_id() {
            tracing::debug!(session.id = %session_id, "reusing active session for router turn");
            Ok(session_id)
        } else {
            self.init_new_session(prompt).await
        }
    }
}

fn git_snapshot(cwd: &Path) -> (Option<String>, Option<String>) {
    let Ok(repo) = gix::discover(cwd) else {
        return (None, None);
    };
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .and_then(|name| name.shorten().to_str().ok().map(str::to_string));
    let diff = git_diff_headers(&repo);

    (branch, diff)
}

fn git_diff_headers(repo: &gix::Repository) -> Option<String> {
    let mut paths = BTreeSet::new();
    let status = repo
        .status(gix::progress::Discard)
        .ok()?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<gix::bstr::BString>::new())
        .ok()?;

    for item in status.flatten() {
        let Ok(path) = item.location().to_str() else {
            continue;
        };
        paths.insert(path.to_string());
    }

    let mut diff = String::new();
    for path in paths {
        writeln!(&mut diff, "diff --git a/{path} b/{path}").expect("writing to String cannot fail");
    }
    (!diff.is_empty()).then_some(diff)
}

async fn load_context_files(cwd: &Path, paths: &[PathBuf]) -> Vec<ContextFile> {
    let mut files = Vec::new();
    for path in paths {
        if let Some(file) = load_context_file(cwd, path).await {
            files.push(file);
        }
    }
    files
}

async fn load_context_file(cwd: &Path, path: &Path) -> Option<ContextFile> {
    let read_path = resolve_workspace_path(cwd, path);
    let content = match tokio::fs::read_to_string(&read_path).await {
        Ok(content) => content,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error.message = %err,
                "failed to read referenced context file"
            );
            return None;
        }
    };

    Some(ContextFile {
        path: path.to_path_buf(),
        content_hash: content_hash(&content),
        content,
        required: true,
    })
}

fn resolve_workspace_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

async fn load_instructions(cwd: &Path) -> Vec<ContextInstruction> {
    let path = cwd.join(AGENTS_MD);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => vec![ContextInstruction {
            source: AGENTS_MD.to_string(),
            content,
        }],
        Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error.message = %err,
                "failed to read workspace instructions"
            );
            Vec::new()
        }
    }
}

fn load_available_skills(store: &SkillStore) -> Vec<Skill> {
    store
        .names()
        .filter_map(|name| match store.load(name) {
            Ok(skill) => Some(skill),
            Err(err) => {
                tracing::warn!(
                    skill.name = %name,
                    error.message = %err,
                    "failed to load discovered skill"
                );
                None
            }
        })
        .collect()
}

fn content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut hash = String::with_capacity("sha256:".len() + 64);
    hash.push_str("sha256:");
    for byte in digest {
        write!(&mut hash, "{byte:02x}").expect("writing to String cannot fail");
    }
    hash
}
