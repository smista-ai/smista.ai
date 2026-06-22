//! Request signals the normalizer extracts for routing.
//!
//! Pure, deterministic readers over the workspace snapshot and the attached
//! skills: the available context kinds a rule's `requires_any_context` matches,
//! the files the task touches, and the skills relevant to the prompt. Also
//! [`paths_match`], the routing-rule path-glob test the router owns (rather than
//! `smista-core`, which holds the policy *data*, not its evaluation).

use std::collections::HashSet;
use std::path::PathBuf;

use globset::{Glob, GlobSetBuilder};
use smista_core::api::Workspace;

/// Derives the available context kinds from the workspace snapshot.
///
/// Each present part of the snapshot contributes one named kind. `pull_request`
/// has no workspace field yet, so it is simply never available.
pub(super) fn context_kinds(workspace: &Workspace) -> HashSet<&'static str> {
    let mut kinds = HashSet::new();
    if workspace.git_diff.is_some() {
        kinds.insert("git_diff");
    }
    if workspace.git_branch.is_some() {
        kinds.insert("git_branch");
    }
    if workspace.active_file.is_some() {
        kinds.insert("active_file");
    }
    if !workspace.referenced_paths.is_empty() {
        kinds.insert("referenced_paths");
    }
    kinds
}

/// Collects the files relevant to the task, de-duplicated and in a stable order.
///
/// The union of the explicitly referenced paths, the active file, and the paths
/// named in the git-diff headers, keeping the first occurrence of each.
pub(super) fn touched_files(workspace: &Workspace) -> Vec<PathBuf> {
    let candidates = workspace
        .referenced_paths
        .iter()
        .cloned()
        .chain(workspace.active_file.clone())
        .chain(workspace.git_diff.iter().flat_map(|diff| diff_paths(diff)));

    let mut seen = HashSet::new();
    candidates
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// Extracts the target paths from unified-diff `diff --git a/… b/…` headers.
fn diff_paths(diff: &str) -> Vec<PathBuf> {
    diff.lines()
        .filter_map(|line| line.strip_prefix("diff --git "))
        .filter_map(|rest| rest.split_once(" b/"))
        .map(|(_, path)| PathBuf::from(path))
        .collect()
}

/// Returns `true` when any of `paths` matches any of the `globs`.
///
/// The router compiles the globs itself, so it owns routing-rule path matching
/// end to end. An empty `globs` slice never matches, and an invalid glob is
/// treated as matching nothing rather than panicking.
pub(super) fn paths_match(globs: &[String], paths: &[PathBuf]) -> bool {
    if globs.is_empty() {
        return false;
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in globs {
        let Ok(glob) = Glob::new(pattern) else {
            tracing::warn!(
                glob.pattern = %pattern,
                "invalid routing glob {{glob.pattern}}; treating as no match"
            );
            return false;
        };
        builder.add(glob);
    }
    let Ok(set) = builder.build() else {
        return false;
    };
    paths.iter().any(|path| set.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        Workspace {
            root: PathBuf::from("/repo"),
            git_branch: None,
            git_diff: None,
            referenced_paths: Vec::new(),
            active_file: None,
        }
    }

    #[test]
    fn should_derive_context_kinds_from_present_fields() {
        let mut ws = workspace();
        ws.git_diff = Some("diff".to_string());
        ws.active_file = Some(PathBuf::from("a.rs"));

        let kinds = context_kinds(&ws);
        assert!(kinds.contains("git_diff"));
        assert!(kinds.contains("active_file"));
        assert!(!kinds.contains("git_branch"));
    }

    #[test]
    fn should_union_touched_files_deduped_and_stable() {
        let mut ws = workspace();
        ws.referenced_paths = vec![PathBuf::from("src/auth/login.rs")];
        ws.active_file = Some(PathBuf::from("src/auth/login.rs"));
        ws.git_diff = Some(
            "diff --git a/src/main.rs b/src/main.rs\ndiff --git a/Cargo.toml b/Cargo.toml"
                .to_string(),
        );

        assert_eq!(
            touched_files(&ws),
            [
                PathBuf::from("src/auth/login.rs"),
                PathBuf::from("src/main.rs"),
                PathBuf::from("Cargo.toml"),
            ]
        );
    }

    #[test]
    fn should_match_paths_against_globs() {
        let globs = vec!["src/auth/**".to_string()];
        assert!(paths_match(&globs, &[PathBuf::from("src/auth/login.rs")]));
        assert!(!paths_match(&globs, &[PathBuf::from("src/main.rs")]));
    }

    #[test]
    fn should_treat_empty_or_invalid_globs_as_no_match() {
        let paths = [PathBuf::from("src/auth/login.rs")];
        assert!(!paths_match(&[], &paths));
        assert!(!paths_match(&["[".to_string()], &paths));
    }
}
