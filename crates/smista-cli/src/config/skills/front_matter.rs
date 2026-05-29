//! Parsing of a `SKILL.md` into its YAML front matter and Markdown body.

use serde::Deserialize;

/// Machine-readable front matter parsed from a `SKILL.md`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct FrontMatter {
    /// Declared skill name; advisory, expected to equal the directory name.
    #[serde(default)]
    pub(super) name: Option<String>,
    /// Human-readable summary of what the skill does.
    #[serde(default)]
    pub(super) description: Option<String>,
}

/// Parses front matter text, treating blank input as empty metadata.
///
/// An empty document is not valid YAML for a struct, so it is short-circuited
/// to default (all-absent) metadata.
pub(super) fn parse_front_matter(raw: &str) -> Result<FrontMatter, yaml_serde::Error> {
    if raw.trim().is_empty() {
        return Ok(FrontMatter::default());
    }
    yaml_serde::from_str(raw).map_err(|source| {
        tracing::error!(
            error.message = %source,
            "failed to parse skill front matter YAML"
        );
        source
    })
}

/// Splits a `SKILL.md` into its YAML front matter and Markdown body.
///
/// Returns `None` when the content does not open with a `---` fence on its
/// first line. The body is everything after the closing `---` fence.
pub(super) fn split_front_matter(contents: &str) -> Option<(&str, &str)> {
    let result = split_front_matter_inner(contents);
    if result.is_some() {
        tracing::trace!("found YAML front matter fence");
    } else {
        tracing::trace!("no YAML front matter fence found");
    }
    result
}

/// Performs the front-matter split; [`split_front_matter`] traces the outcome.
fn split_front_matter_inner(contents: &str) -> Option<(&str, &str)> {
    let after_open = contents
        .strip_prefix("---\n")
        .or_else(|| contents.strip_prefix("---\r\n"))?;
    let mut search_from = 0;
    loop {
        let offset = after_open[search_from..].find("---")?;
        let fence = search_from + offset;
        let at_line_start = fence == 0 || after_open.as_bytes()[fence - 1] == b'\n';
        let after_fence = &after_open[fence + 3..];
        let ends_line = after_fence.is_empty()
            || after_fence.starts_with('\n')
            || after_fence.starts_with('\r');
        if at_line_start && ends_line {
            let front_matter = &after_open[..fence];
            let body = after_fence
                .strip_prefix("\r\n")
                .or_else(|| after_fence.strip_prefix('\n'))
                .unwrap_or(after_fence);
            return Some((front_matter, body));
        }
        search_from = fence + 3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_split_front_matter_from_body() {
        let contents = "---\nname: foo\ndescription: bar\n---\nDo the thing.\n";
        let (front, body) = split_front_matter(contents).unwrap();
        assert_eq!(front, "name: foo\ndescription: bar\n");
        assert_eq!(body, "Do the thing.\n");
    }

    #[test]
    fn should_return_none_when_no_front_matter() {
        assert!(split_front_matter("no front matter here").is_none());
    }

    #[test]
    fn should_treat_blank_front_matter_as_empty_metadata() {
        let meta = parse_front_matter("   \n").unwrap();
        assert!(meta.name.is_none());
        assert!(meta.description.is_none());
    }
}
