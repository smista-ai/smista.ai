//! Discovery and lookup of skills, the source of truth for skill names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use smista_sdk::core::skill::Skill;

use super::front_matter::{parse_front_matter, split_front_matter};
use super::{SkillEntry, SkillError, SkillWarning};
use crate::skills::{global_skills_dir, skills_dir};

/// The set of discovered skills and the source of truth for skill names.
///
/// Built by [`SkillStore::discover`]; the behavioral body of each skill is read
/// lazily through [`SkillStore::load`].
#[derive(Debug, Clone, Default)]
pub struct SkillStore {
    skills: BTreeMap<String, SkillEntry>,
    warnings: Vec<SkillWarning>,
}

impl SkillStore {
    /// Discovers skills for `cwd`, project location first then global.
    ///
    /// Project skills take precedence: a name already discovered in the project
    /// location is never overwritten by a global skill, and the global skill's
    /// descriptor is not read at all. Missing skills roots contribute nothing.
    #[must_use]
    pub fn discover(cwd: &Path) -> Self {
        let mut store = Self::default();
        // ingest is resilient to missing roots, so we can just call it on both without
        [Some(skills_dir(cwd)), global_skills_dir()]
            .into_iter()
            .flatten()
            .for_each(|dir| store.ingest(&dir));

        tracing::debug!(
            skills.count = store.skills.len(),
            skills.warnings = store.warnings.len(),
            "skill discovery complete"
        );
        store
    }

    /// Reads `path`'s immediate subdirectories as skills, skipping any name that
    /// is already known (preserving earlier-root precedence and avoiding IO).
    fn ingest(&mut self, path: &Path) {
        tracing::trace!(skills.root = %path.display(), "ingesting skills root");
        let Ok(entries) = std::fs::read_dir(path) else {
            // A missing or unreadable skills root simply contributes nothing.
            tracing::trace!(skills.root = %path.display(), "skills root is absent or unreadable");
            return;
        };

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(name) = dir.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                // Non-UTF-8 directory names cannot be skill identifiers.
                continue;
            };
            // Earlier paths win; skip without touching the filesystem.
            if self.skills.contains_key(&name) {
                continue;
            }
            self.ingest_dir(name, dir);
        }
    }

    /// Reads one skill directory's metadata into the store, recording a warning
    /// instead when the directory is not a usable skill.
    fn ingest_dir(&mut self, name: String, dir: PathBuf) {
        let skill_file = dir.join(super::SKILL_FILE);

        let Ok(contents) = std::fs::read_to_string(&skill_file) else {
            tracing::warn!(
                skill.name = %name,
                skill.dir = %dir.display(),
                "skill directory has no SKILL.md; ignoring"
            );
            self.warnings.push(SkillWarning::MissingSkillFile { dir });
            return;
        };

        let raw = match split_front_matter(&contents) {
            Some((front_matter, _body)) => front_matter,
            None => {
                tracing::warn!(
                    skill.name = %name,
                    skill.dir = %dir.display(),
                    "skill has no YAML front matter; ignoring"
                );
                self.warnings.push(SkillWarning::InvalidFrontMatter {
                    dir,
                    reason: "missing YAML front matter".to_string(),
                });
                return;
            }
        };

        let meta = match parse_front_matter(raw) {
            Ok(meta) => meta,
            Err(source) => {
                tracing::warn!(
                    skill.name = %name,
                    skill.dir = %dir.display(),
                    error.message = %source,
                    "skill has invalid front matter; ignoring"
                );
                self.warnings.push(SkillWarning::InvalidFrontMatter {
                    dir,
                    reason: source.to_string(),
                });
                return;
            }
        };

        if let Some(declared) = &meta.name
            && declared != &name
        {
            tracing::warn!(
                skill.name = %name,
                skill.declared = %declared,
                skill.dir = %dir.display(),
                "skill front matter name differs from directory name"
            );
            self.warnings.push(SkillWarning::NameMismatch {
                dir: dir.clone(),
                declared: declared.clone(),
            });
        }

        let description = meta.description.unwrap_or_default();
        if description.is_empty() {
            tracing::warn!(
                skill.name = %name,
                skill.dir = %dir.display(),
                "skill is missing a description"
            );
            self.warnings
                .push(SkillWarning::MissingDescription { dir: dir.clone() });
        }

        tracing::debug!(
            skill.name = %name,
            skill.dir = %dir.display(),
            "discovered skill"
        );
        self.skills.insert(name, SkillEntry::new(dir, description));
    }

    /// Reads a discovered skill's `SKILL.md` and builds a [`Skill`].
    ///
    /// The Markdown body becomes the skill's behavioral instructions; it is read
    /// here, on demand, rather than being held in memory after discovery. The
    /// returned [`Skill::name`] is the directory name (the canonical identity),
    /// not necessarily the front matter `name`.
    ///
    /// # Errors
    ///
    /// Returns [`SkillError::NotFound`] when no skill named `name` was
    /// discovered, [`SkillError::Io`] when the descriptor cannot be read, or
    /// [`SkillError::Parse`] when its front matter is not valid YAML.
    pub fn load(&self, name: &str) -> Result<Skill, SkillError> {
        tracing::debug!(skill.name = %name, "loading skill");
        let entry = self.skills.get(name).ok_or_else(|| SkillError::NotFound {
            name: name.to_string(),
        })?;

        let skill_file = entry.skill_file();

        let contents = std::fs::read_to_string(&skill_file).map_err(|source| SkillError::Io {
            path: skill_file.display().to_string(),
            source,
        })?;

        let (raw, body) = split_front_matter(&contents).unwrap_or(("", contents.as_str()));

        // Parse the front matter to validate it; the description is read at
        // discovery time, so only the body is needed for the loaded skill.
        parse_front_matter(raw).map_err(|source| SkillError::Parse {
            path: skill_file.display().to_string(),
            source,
        })?;

        Ok(Skill {
            name: name.to_string(),
            content: body.trim().to_string(),
        })
    }

    /// Returns an iterator over the discovered skill names.
    ///
    /// This is the source of truth other components validate skill references
    /// against.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.skills.keys().map(String::as_str)
    }

    /// Returns whether a skill with `name` was discovered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Returns the entry for `name`, if discovered.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.get(name)
    }

    /// Returns the discovery warnings collected while building the store.
    #[must_use]
    pub fn warnings(&self) -> &[SkillWarning] {
        &self.warnings
    }

    /// Returns the number of discovered skills.
    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Returns whether no skills were discovered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("smista-skills-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_skill(root: &Path, dir_name: &str, front: &str, body: &str) {
        let dir = root.join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        let contents = format!("---\n{front}\n---\n{body}");
        std::fs::write(dir.join(super::super::SKILL_FILE), contents).unwrap();
    }

    #[test]
    fn should_be_empty_when_root_missing() {
        let mut store = SkillStore::default();
        store.ingest(Path::new("/this/path/does/not/exist"));
        assert!(store.is_empty());
        assert!(store.warnings().is_empty());
    }

    #[test]
    fn should_discover_skill_with_front_matter() {
        let root = temp_root("discover");
        write_skill(
            &root,
            "rust-conventions",
            "name: rust-conventions\ndescription: Enforce Rust style.",
            "Follow the project rustfmt config.",
        );
        let mut store = SkillStore::default();
        store.ingest(&root);
        assert!(store.contains("rust-conventions"));
        assert_eq!(
            store.get("rust-conventions").unwrap().description(),
            "Enforce Rust style."
        );
        assert!(store.warnings().is_empty());
    }

    #[test]
    fn should_warn_and_skip_directory_without_skill_file() {
        let root = temp_root("no-skill-md");
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let mut store = SkillStore::default();
        store.ingest(&root);
        assert!(!store.contains("empty"));
        assert!(matches!(
            store.warnings().first(),
            Some(SkillWarning::MissingSkillFile { .. })
        ));
    }

    #[test]
    fn should_prefer_first_root_on_name_clash() {
        let project = temp_root("clash-project");
        let global = temp_root("clash-global");
        write_skill(
            &project,
            "shared",
            "name: shared\ndescription: project version",
            "Project body.",
        );
        write_skill(
            &global,
            "shared",
            "name: shared\ndescription: global version",
            "Global body.",
        );
        let mut store = SkillStore::default();
        store.ingest(&project);
        store.ingest(&global);
        // Project wins, and the global descriptor was never read.
        assert_eq!(
            store.get("shared").unwrap().description(),
            "project version"
        );
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn should_load_instructions_from_body() {
        let root = temp_root("load-body");
        write_skill(
            &root,
            "code-review",
            "name: code-review\ndescription: Review changes.",
            "Report findings by severity.\n",
        );
        let mut store = SkillStore::default();
        store.ingest(&root);
        let skill = store.load("code-review").unwrap();
        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.content, "Report findings by severity.");
    }

    #[test]
    fn should_report_not_found_for_unknown_skill() {
        let store = SkillStore::default();
        assert!(matches!(
            store.load("missing"),
            Err(SkillError::NotFound { .. })
        ));
    }

    #[test]
    fn should_warn_on_name_mismatch() {
        let root = temp_root("mismatch");
        write_skill(
            &root,
            "actual-dir",
            "name: declared-name\ndescription: Mismatched.",
            "Body.",
        );
        let mut store = SkillStore::default();
        store.ingest(&root);
        // Identity is the directory name, not the declared name.
        assert!(store.contains("actual-dir"));
        assert!(!store.contains("declared-name"));
        assert!(matches!(
            store.warnings().first(),
            Some(SkillWarning::NameMismatch { .. })
        ));
    }

    #[test]
    fn should_warn_when_description_missing() {
        let root = temp_root("no-desc");
        write_skill(&root, "skill", "name: skill", "Body.");
        let mut store = SkillStore::default();
        store.ingest(&root);
        assert!(store.contains("skill"));
        assert!(matches!(
            store.warnings().first(),
            Some(SkillWarning::MissingDescription { .. })
        ));
    }
}
