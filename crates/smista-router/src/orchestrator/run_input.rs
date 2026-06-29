//! The run request-context snapshot, split into a clear and a sealable half.
//!
//! A `continue` carries only the pause answer, but every turn re-runs the
//! deterministic resolver, which needs the original policy, preferences,
//! workspace and input. The orchestrator persists that context once at run
//! start. This module splits an [`ExecuteRequest`] into the non-secret
//! [`RunInputMeta`] (stored in clear) and the sealable [`RunInputBundle`] (the
//! input text, attachments and git diff), and rebuilds the [`Workspace`] from
//! the two when a later turn recalls them.
use serde::{Deserialize, Serialize};
use smista_core::api::{
    Attachments, ExecutePolicy, ExecuteRequest, LocalPreferences, TaskInput, Workspace,
};
use smista_storage::entity::SessionRunInput;

use crate::orchestrator::error::OrchestratorError;

/// Current schema version of a persisted [`RunInputBundle`].
const BUNDLE_VERSION: u8 = 1;

/// The sealable half of a run's request context.
///
/// Holds everything that may contain user content — the input text, the
/// attachments and the git diff — so it can be sealed for an encrypted session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RunInputBundle {
    /// Schema version, so the persisted form can evolve.
    pub(crate) version: u8,
    /// The user's request: prompt text, optional command and explicit model.
    pub(crate) input: TaskInput,
    /// Local content the router cannot read for itself.
    pub(crate) attachments: Attachments,
    /// The workspace git diff, separated out because it may contain secrets.
    pub(crate) git_diff: Option<String>,
}

impl RunInputBundle {
    /// A minimal bundle carrying only injected user text.
    ///
    /// An `inject` redirects the run with fresh input; it does not carry the
    /// original attachments or diff, so this drops them and keeps just the text.
    pub(crate) fn for_injection(text: String) -> Self {
        Self {
            version: BUNDLE_VERSION,
            input: TaskInput {
                text,
                command: None,
                explicit_model: None,
            },
            attachments: Attachments {
                files: Vec::new(),
                instructions: Vec::new(),
                invoked_skills: Vec::new(),
                available_skills: Vec::new(),
            },
            git_diff: None,
        }
    }
}

/// The non-secret half of a run's request context, stored in clear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RunInputMeta {
    /// The deterministic policy snapshot.
    pub(crate) policy: ExecutePolicy,
    /// The client's local execution preferences.
    pub(crate) local_preferences: LocalPreferences,
    /// The workspace, minus its (secret) git diff.
    pub(crate) workspace: WorkspaceMeta,
}

/// The non-secret fields of a [`Workspace`]: paths and flags, no git diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceMeta {
    /// Absolute path to the workspace root.
    pub(crate) root: std::path::PathBuf,
    /// Current git branch, if the workspace is a git repository.
    pub(crate) git_branch: Option<String>,
    /// Paths the user explicitly referenced.
    pub(crate) referenced_paths: Vec<std::path::PathBuf>,
    /// The file the user is actively editing, if any.
    pub(crate) active_file: Option<std::path::PathBuf>,
}

/// Splits an [`ExecuteRequest`] into its non-secret meta and sealable bundle.
///
/// The git diff is moved out of the workspace and into the bundle, so the meta
/// half carries only paths and flags that are safe to store in clear.
pub(crate) fn split_execute_request(request: ExecuteRequest) -> (RunInputMeta, RunInputBundle) {
    let ExecuteRequest {
        input,
        workspace,
        policy,
        local_preferences,
        attachments,
    } = request;

    let Workspace {
        root,
        git_branch,
        git_diff,
        referenced_paths,
        active_file,
    } = workspace;

    let meta = RunInputMeta {
        policy,
        local_preferences,
        workspace: WorkspaceMeta {
            root,
            git_branch,
            referenced_paths,
            active_file,
        },
    };
    let bundle = RunInputBundle {
        version: BUNDLE_VERSION,
        input,
        attachments,
        git_diff,
    };
    (meta, bundle)
}

/// Rebuilds the non-secret [`RunInputMeta`] from a recalled run-input row.
///
/// The metadata halves are always stored in clear, so this never needs the
/// sealable bundle; a continuation rebuilds the meta up front and defers the
/// bundle to the turn, which opens it (decrypting if sealed) when it runs.
pub(crate) fn rebuild_run_meta(input: &SessionRunInput) -> Result<RunInputMeta, OrchestratorError> {
    let policy = serde_json::from_str(&input.policy).map_err(|error| {
        OrchestratorError::Internal(format!("run-input policy decode: {error}"))
    })?;
    let local_preferences = serde_json::from_str(&input.local_preferences).map_err(|error| {
        OrchestratorError::Internal(format!("run-input preferences decode: {error}"))
    })?;
    let workspace = serde_json::from_str(&input.workspace).map_err(|error| {
        OrchestratorError::Internal(format!("run-input workspace decode: {error}"))
    })?;
    Ok(RunInputMeta {
        policy,
        local_preferences,
        workspace,
    })
}

/// Rebuilds the full [`Workspace`] from a recalled meta and bundle.
pub(crate) fn rebuild_workspace(meta: &RunInputMeta, bundle: &RunInputBundle) -> Workspace {
    Workspace {
        root: meta.workspace.root.clone(),
        git_branch: meta.workspace.git_branch.clone(),
        git_diff: bundle.git_diff.clone(),
        referenced_paths: meta.workspace.referenced_paths.clone(),
        active_file: meta.workspace.active_file.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use smista_core::intent::TaskIntent;
    use smista_core::policy::{ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig};

    use super::*;

    fn sample_execute_request() -> ExecuteRequest {
        ExecuteRequest {
            input: TaskInput {
                text: "refactor".to_string(),
                command: Some(TaskIntent::Edit),
                explicit_model: None,
            },
            workspace: Workspace {
                root: PathBuf::from("/repo"),
                git_branch: Some("main".to_string()),
                git_diff: Some("diff --git a b".to_string()),
                referenced_paths: vec![PathBuf::from("src/lib.rs")],
                active_file: None,
            },
            policy: ExecutePolicy {
                version: 1,
                source: "merged".to_string(),
                classification: ClassificationConfig::default(),
                routing: RoutingPolicy::default(),
                tools: ToolsConfig::default(),
                privacy: PrivacyPolicy::default(),
            },
            local_preferences: LocalPreferences {
                auto_apply: false,
                stream: true,
                local_only: false,
                no_network: false,
            },
            attachments: Attachments {
                files: Vec::new(),
                instructions: Vec::new(),
                invoked_skills: Vec::new(),
                available_skills: Vec::new(),
            },
        }
    }

    #[test]
    fn should_split_and_rebuild_request_context() {
        let request = sample_execute_request();
        let original = request.clone();
        let (meta, bundle) = split_execute_request(request);

        assert_eq!(bundle.version, 1);
        assert_eq!(meta.workspace.git_branch, original.workspace.git_branch);
        assert_eq!(bundle.git_diff, original.workspace.git_diff);

        let workspace = rebuild_workspace(&meta, &bundle);
        assert_eq!(workspace.git_diff, original.workspace.git_diff);
        assert_eq!(
            workspace.referenced_paths,
            original.workspace.referenced_paths
        );
        assert_eq!(workspace, original.workspace);
    }
}
