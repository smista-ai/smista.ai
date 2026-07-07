//! Local execution for client-side tools.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, anyhow, bail};
use serde_json::Value;
use smista_sdk::core::api::{ApprovalDecision, ToolResult};

const READ_FILE_TOOL: &str = "read_file";
const WRITE_FILE_TOOL: &str = "write_file";
const EDIT_FILE_TOOL: &str = "edit_file";
const SHELL_TOOL: &str = "shell";

/// A tool call ready to execute on the user's machine.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// Identifier matching the originating router request.
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// Tool arguments as provider-neutral JSON.
    pub arguments: Value,
    /// Approval decision to include with ask-mode tool results.
    pub decision: Option<ApprovalDecision>,
}

/// Executes built-in tools against a workspace root.
///
/// File tools reject paths that normalize outside the workspace. Shell commands
/// run with the workspace as their current directory, and their command text is
/// intentionally not written to logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutor {
    cwd: PathBuf,
}

impl ToolExecutor {
    /// Creates a tool executor rooted at `cwd`.
    #[must_use]
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Executes `call` and converts failures into tool-result errors.
    pub async fn execute(&self, call: ToolCall) -> ToolResult {
        let call_id = call.call_id.clone();
        let name = call.name.clone();
        let decision = call.decision;
        tracing::debug!(
            tool.call_id = %call_id,
            tool.name = %name,
            tool.approved = decision.is_some(),
            "executing local tool"
        );
        let outcome = match call.name.as_str() {
            READ_FILE_TOOL => self.read_file(&call.arguments).await,
            WRITE_FILE_TOOL => self.write_file(&call.arguments).await,
            EDIT_FILE_TOOL => self.edit_file(&call.arguments).await,
            SHELL_TOOL => self.run_shell(&call.arguments).await,
            name => Err(anyhow!("unsupported tool: {name}")),
        };

        match outcome {
            Ok(content) => {
                tracing::debug!(
                    tool.call_id = %call_id,
                    tool.name = %name,
                    output.bytes = content.len(),
                    "local tool completed"
                );
                ToolResult {
                    call_id,
                    content,
                    is_error: false,
                    decision,
                }
            }
            Err(error) => {
                tracing::warn!(
                    tool.call_id = %call_id,
                    tool.name = %name,
                    "local tool failed: {error}"
                );
                ToolResult {
                    call_id,
                    content: error.to_string(),
                    is_error: true,
                    decision,
                }
            }
        }
    }

    async fn read_file(&self, arguments: &Value) -> anyhow::Result<String> {
        let path = self.resolve_path(required_string(arguments, "path")?)?;
        tracing::debug!(file.path = %path.display(), "reading file for local tool");
        let content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        tracing::debug!(
            file.path = %path.display(),
            file.bytes = content.len(),
            "file read for local tool"
        );
        Ok(content)
    }

    async fn write_file(&self, arguments: &Value) -> anyhow::Result<String> {
        let path = self.resolve_path(required_string(arguments, "path")?)?;
        let content = required_string(arguments, "content")?;
        tracing::debug!(
            file.path = %path.display(),
            file.bytes = content.len(),
            "writing file for local tool"
        );
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        tracing::debug!(file.path = %path.display(), "file written for local tool");

        Ok(format!("wrote {}", path.display()))
    }

    async fn edit_file(&self, arguments: &Value) -> anyhow::Result<String> {
        let path = self.resolve_path(required_string(arguments, "path")?)?;
        let old = required_string(arguments, "old")?;
        let new = required_string(arguments, "new")?;
        tracing::debug!(
            file.path = %path.display(),
            old.bytes = old.len(),
            new.bytes = new.len(),
            "editing file for local tool"
        );
        let content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        if !content.contains(old) {
            bail!("old text was not found in {}", path.display());
        }
        let updated = content.replacen(old, new, 1);
        tokio::fs::write(&path, updated)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        tracing::debug!(file.path = %path.display(), "file edited for local tool");

        Ok(format!("edited {}", path.display()))
    }

    async fn run_shell(&self, arguments: &Value) -> anyhow::Result<String> {
        let command = required_string(arguments, "command")?.to_owned();
        let cwd = self.cwd.clone();
        tracing::debug!(
            shell.command.bytes = command.len(),
            shell.cwd = %cwd.display(),
            "running shell command for local tool"
        );
        tokio::task::spawn_blocking(move || run_shell_blocking(&cwd, &command))
            .await
            .context("failed to join shell task")?
    }

    fn resolve_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        let root = normalize_path(&self.cwd);
        let path = PathBuf::from(path);
        let candidate = if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        };
        let normalized = normalize_path(&candidate);
        if !normalized.starts_with(&root) {
            tracing::warn!(
                file.path = %normalized.display(),
                workspace.root = %root.display(),
                "rejected local tool path outside workspace"
            );
            bail!(
                "path {} is outside the workspace {}",
                normalized.display(),
                root.display()
            );
        }

        Ok(normalized)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn required_string<'a>(arguments: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string argument `{field}`"))
}

fn run_shell_blocking(cwd: &Path, command: &str) -> anyhow::Result<String> {
    #[cfg(target_family = "windows")]
    let output = Command::new("cmd")
        .args(["/C", command])
        .current_dir(cwd)
        .output()
        .context("failed to run shell command")?;

    #[cfg(not(target_family = "windows"))]
    let output = Command::new("sh")
        .args(["-c", command])
        .current_dir(cwd)
        .output()
        .context("failed to run shell command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let content = if stderr.is_empty() {
        stdout.into_owned()
    } else if stdout.is_empty() {
        stderr.into_owned()
    } else {
        format!("{stdout}{stderr}")
    };

    tracing::debug!(
        shell.command.bytes = command.len(),
        shell.status = %output.status,
        stdout.bytes = output.stdout.len(),
        stderr.bytes = output.stderr.len(),
        "shell command completed for local tool"
    );

    if output.status.success() {
        Ok(content)
    } else {
        bail!(
            "shell command exited with {status}: {content}",
            status = output.status
        )
    }
}
