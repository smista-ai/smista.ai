//! Shared glob compilation for path-based policy matching.

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Compiles a set of glob patterns into a [`GlobSet`].
///
/// Used by routing rules and privacy checks. Invalid patterns return an error,
/// which configuration validation surfaces; callers in the hot path treat a
/// failed compile conservatively rather than panicking.
pub fn compile_globs(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    tracing::trace!(glob.count = patterns.len(), "compiling glob patterns");
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).inspect_err(|source| {
            tracing::warn!(
                glob.pattern = %pattern,
                error.message = %source,
                "invalid glob pattern; compilation failed"
            );
        })?;
        builder.add(glob);
    }
    builder.build().inspect_err(|source| {
        tracing::error!(
            glob.count = patterns.len(),
            error.message = %source,
            "failed to build glob set"
        );
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn should_build_empty_set_that_matches_nothing() {
        let set = compile_globs(&[]).unwrap();
        assert!(!set.is_match(Path::new("anything")));
    }

    #[test]
    fn should_error_on_invalid_glob() {
        assert!(compile_globs(&["[".to_string()]).is_err());
    }

    #[test]
    fn should_match_compiled_globs() {
        let set = compile_globs(&["src/auth/**".to_string(), "*.pem".to_string()]).unwrap();
        assert!(set.is_match(Path::new("src/auth/login.rs")));
        assert!(set.is_match(Path::new("key.pem")));
        assert!(!set.is_match(Path::new("src/main.rs")));
    }
}
