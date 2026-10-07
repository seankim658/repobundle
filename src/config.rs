use std::fs;
use std::io;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use thiserror::Error;
use tracing::debug;

use crate::git::RefSelection;
use crate::naming::NameTemplate;

const CONFIG_FILE_NAME: &str = ".repobundle.toml";

/// The `[defaults]` table that the global and project config files share.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Expand a leading `~` to the home directory, and resolve relative paths against the repo
    /// root.
    pub output: Option<PathBuf>,
    pub name: Option<NameTemplate>,
    pub refs: Option<RefSelection>,
    pub prune: Option<NonZeroUsize>,
    pub max_size_mb: Option<NonZeroU64>,
    pub repo_name: Option<RepoName>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    defaults: Defaults,
}

impl Defaults {
    /// Fill each unset value from `fallback`.
    fn or(self, fallback: Self) -> Self {
        Self {
            output: self.output.or(fallback.output),
            name: self.name.or(fallback.name),
            refs: self.refs.or(fallback.refs),
            prune: self.prune.or(fallback.prune),
            max_size_mb: self.max_size_mb.or(fallback.max_size_mb),
            repo_name: self.repo_name.or(fallback.repo_name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct RepoName(String);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RepoNameError {
    #[error("repo name is empty")]
    Empty,
    #[error("repo name `{0}` must not contain path separators")]
    PathSeparator(String),
}

impl RepoName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoName {
    type Error = RepoNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            return Err(RepoNameError::Empty);
        }
        if name.contains(['/', '\\']) {
            return Err(RepoNameError::PathSeparator(name));
        }
        Ok(Self(name))
    }
}

/// Read one config file, treating a missing file as one with no values set.
pub fn load_file(path: &Path) -> Result<Defaults> {
    let Some(text) = read_if_exists(path)? else {
        debug!(path = %path.display(), "no config file");
        return Ok(Defaults::default());
    };
    debug!(path = %path.display(), "loading config file");
    parse(&text).with_context(|| format!("config file {} is malformed", path.display()))
}

/// Load the global and project config files, with project values taking precedence.
/// Return `output` already resolved to a full path.
pub fn load(start: &Path, repo_root: &Path, home: Option<&Path>) -> Result<Defaults> {
    let global = match home {
        Some(home) => load_file(&global_file(home))?,
        None => Defaults::default(),
    };
    let project = match find_project_file(start, repo_root, home)? {
        Some(path) => load_file(&path)?,
        None => {
            debug!(root = %repo_root.display(), "no project config file");
            Defaults::default()
        }
    };
    let mut merged = project.or(global);
    merged.output = merged
        .output
        .map(|output| resolve_output(&output, repo_root, home))
        .transpose()?;
    Ok(merged)
}

pub fn global_file(home: &Path) -> PathBuf {
    home.join(CONFIG_FILE_NAME)
}

/// Return the project file closest to `start`, searching up to and including `repo_root`.
pub fn find_project_file(
    start: &Path,
    repo_root: &Path,
    home: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let start = canonicalize(start)?;
    let repo_root = canonicalize(repo_root)?;
    if !start.starts_with(&repo_root) {
        bail!(
            "{} is not inside the repository at {}",
            start.display(),
            repo_root.display()
        );
    }
    let home = home.and_then(|home| fs::canonicalize(home).ok());
    for dir in start.ancestors() {
        if let Some(path) = project_file_in(dir, home.as_deref()) {
            return Ok(Some(path));
        }
        if dir == repo_root {
            break;
        }
    }
    Ok(None)
}

fn read_if_exists(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read config file {}", path.display()))
        }
    }
}

fn parse(text: &str) -> Result<Defaults, toml::de::Error> {
    let file: ConfigFile = toml::from_str(text)?;
    Ok(file.defaults)
}

fn project_file_in(dir: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if Some(dir) == home {
        return None;
    }
    let path = dir.join(CONFIG_FILE_NAME);
    path.is_file().then_some(path)
}

fn canonicalize(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))
}

/// Expand a leading `~` to `home`, then resolve relative paths against `repo_root`.
fn resolve_output(output: &Path, repo_root: &Path, home: Option<&Path>) -> Result<PathBuf> {
    let Ok(rest) = output.strip_prefix("~") else {
        return Ok(repo_root.join(output));
    };
    let Some(home) = home else {
        bail!(
            "output {} starts with `~`, but the home directory is unknown",
            output.display()
        );
    };
    Ok(home.join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn parse_error(text: &str) -> String {
        parse(text).unwrap_err().to_string()
    }

    fn write_config(dir: &Path) {
        write_config_with(dir, "");
    }

    fn write_config_with(dir: &Path, text: &str) {
        fs::write(dir.join(CONFIG_FILE_NAME), text).unwrap();
    }

    fn load_from(root: &TempDir, home: &TempDir) -> Defaults {
        load(root.path(), root.path(), Some(home.path())).unwrap()
    }

    fn canonical_config_path(dir: &Path) -> PathBuf {
        fs::canonicalize(dir).unwrap().join(CONFIG_FILE_NAME)
    }

    // Parsing

    #[test]
    fn parses_every_key() {
        let defaults = parse(
            r#"
            [defaults]
            output = "out"
            name = "{date}-{repo}.bundle"
            refs = "branches"
            prune = 3
            max_size_mb = 30
            repo_name = "renamed"
            "#,
        )
        .unwrap();

        let expected = Defaults {
            output: Some(PathBuf::from("out")),
            name: Some("{date}-{repo}.bundle".parse().unwrap()),
            refs: Some(RefSelection::Branches),
            prune: NonZeroUsize::new(3),
            max_size_mb: NonZeroU64::new(30),
            repo_name: Some(RepoName::try_from("renamed".to_string()).unwrap()),
        };
        assert_eq!(defaults, expected);
    }

    #[test]
    fn empty_file_sets_no_values() {
        assert_eq!(parse("").unwrap(), Defaults::default());
        assert_eq!(parse("[defaults]").unwrap(), Defaults::default());
    }

    #[test]
    fn rejects_unknown_key() {
        let error = parse_error("[defaults]\nmax_size = 30");
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn rejects_unknown_table() {
        let error = parse_error("[default]\nprune = 3");
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn rejects_invalid_template() {
        let error = parse_error("[defaults]\nname = \"{author}.bundle\"");
        assert!(error.contains("unknown placeholder"), "{error}");
    }

    #[test]
    fn rejects_unknown_refs_value() {
        let error = parse_error("[defaults]\nrefs = \"everything\"");
        assert!(error.contains("unknown variant"), "{error}");
    }

    #[test]
    fn rejects_zero_prune() {
        assert!(parse("[defaults]\nprune = 0").is_err());
    }

    #[test]
    fn rejects_zero_max_size() {
        assert!(parse("[defaults]\nmax_size_mb = 0").is_err());
    }

    #[test]
    fn accepts_repo_name_with_dots_and_hyphens() {
        let name = RepoName::try_from("my.repo-v2".to_string()).unwrap();
        assert_eq!(name.as_str(), "my.repo-v2");
    }

    #[test]
    fn rejects_empty_repo_name() {
        assert_eq!(RepoName::try_from(String::new()), Err(RepoNameError::Empty));
    }

    #[test]
    fn rejects_repo_name_with_path_separators() {
        for name in ["team/repo", "team\\repo"] {
            assert_eq!(
                RepoName::try_from(name.to_string()),
                Err(RepoNameError::PathSeparator(name.to_string())),
                "{name}"
            );
        }
    }

    #[test]
    fn rejects_invalid_repo_name_in_file() {
        let error = parse_error("[defaults]\nrepo_name = \"team/repo\"");
        assert!(error.contains("path separators"), "{error}");
    }

    #[test]
    fn example_config_sets_every_key() {
        let example = include_str!("../repobundle.example.toml");
        let uncommented = example.replace("# repo_name = ", "repo_name = ");
        assert_ne!(uncommented, example, "repo_name line not found");

        let defaults = parse(&uncommented).unwrap();
        assert!(defaults.output.is_some());
        assert!(defaults.name.is_some());
        assert!(defaults.refs.is_some());
        assert!(defaults.prune.is_some());
        assert!(defaults.max_size_mb.is_some());
        assert!(defaults.repo_name.is_some());
    }

    // Loading files

    #[test]
    fn missing_file_sets_no_values() {
        let dir = TempDir::new().unwrap();
        let defaults = load_file(&dir.path().join("absent.toml")).unwrap();
        assert_eq!(defaults, Defaults::default());
    }

    #[test]
    fn loads_values_from_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[defaults]\nprune = 2").unwrap();
        assert_eq!(load_file(&path).unwrap().prune, NonZeroUsize::new(2));
    }

    #[test]
    fn malformed_file_error_names_file_and_cause() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[defaults]\nprune = \"three\"").unwrap();

        let error = format!("{:#}", load_file(&path).unwrap_err());
        assert!(error.contains(&path.display().to_string()), "{error}");
        assert!(error.contains("prune"), "{error}");
    }

    // Finding files

    #[test]
    fn finds_project_file_at_repo_root() {
        let root = TempDir::new().unwrap();
        write_config(root.path());

        let found = find_project_file(root.path(), root.path(), None).unwrap();
        assert_eq!(found, Some(canonical_config_path(root.path())));
    }

    #[test]
    fn finds_project_file_from_subdirectory() {
        let root = TempDir::new().unwrap();
        let nested = root.path().join("a/b");
        fs::create_dir_all(&nested).unwrap();
        write_config(root.path());

        let found = find_project_file(&nested, root.path(), None).unwrap();
        assert_eq!(found, Some(canonical_config_path(root.path())));
    }

    #[test]
    fn prefers_closest_project_file() {
        let root = TempDir::new().unwrap();
        let middle = root.path().join("a");
        let nested = middle.join("b");
        fs::create_dir_all(&nested).unwrap();
        write_config(root.path());
        write_config(&middle);

        let found = find_project_file(&nested, root.path(), None).unwrap();
        assert_eq!(found, Some(canonical_config_path(&middle)));
    }

    #[test]
    fn ignores_files_above_repo_root() {
        let outer = TempDir::new().unwrap();
        let root = outer.path().join("repo");
        fs::create_dir(&root).unwrap();
        write_config(outer.path());

        assert_eq!(find_project_file(&root, &root, None).unwrap(), None);
    }

    #[test]
    fn skips_config_in_home_directory_used_as_repo() {
        let home = TempDir::new().unwrap();
        write_config(home.path());

        let found = find_project_file(home.path(), home.path(), Some(home.path())).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn finds_project_file_inside_repo_under_home() {
        let home = TempDir::new().unwrap();
        let root = home.path().join("repo");
        fs::create_dir(&root).unwrap();
        write_config(home.path());
        write_config(&root);

        let found = find_project_file(&root, &root, Some(home.path())).unwrap();
        assert_eq!(found, Some(canonical_config_path(&root)));
    }

    #[test]
    fn returns_none_without_project_file() {
        let root = TempDir::new().unwrap();
        assert_eq!(
            find_project_file(root.path(), root.path(), None).unwrap(),
            None
        );
    }

    #[test]
    fn start_outside_repo_is_an_error() {
        let root = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let error = find_project_file(elsewhere.path(), root.path(), None).unwrap_err();
        assert!(error.to_string().contains("is not inside"), "{error}");
    }

    // Loading and merging

    #[test]
    fn no_files_set_no_values() {
        let (root, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        assert_eq!(load_from(&root, &home), Defaults::default());
    }

    #[test]
    fn project_values_override_global_values() {
        let (root, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        write_config_with(home.path(), "[defaults]\nprune = 5\nrefs = \"head\"");
        write_config_with(root.path(), "[defaults]\nprune = 2");

        let defaults = load_from(&root, &home);
        assert_eq!(defaults.prune, NonZeroUsize::new(2));
        assert_eq!(defaults.refs, Some(RefSelection::Head));
    }

    #[test]
    fn relative_output_resolves_against_repo_root() {
        let (root, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        write_config_with(home.path(), "[defaults]\noutput = \"out\"");

        assert_eq!(
            load_from(&root, &home).output,
            Some(root.path().join("out"))
        );
    }

    #[test]
    fn absolute_output_stays_absolute() {
        let (root, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let elsewhere = TempDir::new().unwrap();
        let text = format!("[defaults]\noutput = '{}'", elsewhere.path().display());
        write_config_with(root.path(), &text);

        assert_eq!(
            load_from(&root, &home).output,
            Some(elsewhere.path().to_path_buf())
        );
    }

    #[test]
    fn tilde_output_expands_to_home() {
        let (root, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        write_config_with(home.path(), "[defaults]\noutput = \"~/bundles\"");

        assert_eq!(
            load_from(&root, &home).output,
            Some(home.path().join("bundles"))
        );
    }

    #[test]
    fn tilde_output_without_home_is_an_error() {
        let root = TempDir::new().unwrap();
        write_config_with(root.path(), "[defaults]\noutput = \"~/bundles\"");

        let error = load(root.path(), root.path(), None).unwrap_err();
        assert!(error.to_string().contains("home directory"), "{error}");
    }
}
