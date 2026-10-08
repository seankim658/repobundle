use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use tracing::debug;

use crate::bundles;
use crate::config::RepoName;
use crate::git::{Git, RefSet, RefTip};
use crate::naming::{NameTemplate, NameValues};
use crate::settings::Output;

const TEMP_SUFFIX: &str = ".tmp";
const IGNORE_FILE: &str = ".gitignore";
/// Ignore everything in the directory, this file included.
const IGNORE_ALL: &str = "# Created by repobundle so git ignores the bundles here.\n*\n";

/// Check that the repo can be bundled and return where the new bundle goes. A directory output
/// gets a fresh name stamped with the current time.
pub fn bundle_path(git: &Git, output: &Output, repo: &RepoName, refs: &RefSet) -> Result<PathBuf> {
    git.ensure_has_commits()?;
    git.ensure_not_shallow()?;
    let dir = match output {
        Output::File(path) => return Ok(path.clone()),
        Output::Directory(dir) => dir,
    };
    let hash = git.short_hash()?;
    let values = NameValues {
        created_at: Utc::now(),
        hash: &hash,
        repo: repo.as_str(),
        branch: refs.branch(),
    };
    Ok(dir.path.join(render_name(&dir.name, &values)?))
}

/// Write a verified bundle to `path` and return its size in bytes. Build it under a temporary
/// name first, so a failed create or verify never replaces an existing bundle at `path`.
pub fn write_bundle(git: &Git, path: &Path, refs: &RefSet) -> Result<u64> {
    let temp = temp_path(path)?;
    create_parent_dir(git, path)?;
    if let Err(error) = create_and_verify(git, &temp, refs) {
        return Err(discard(&temp, error));
    }
    if let Err(error) = move_into_place(&temp, path) {
        return Err(discard(&temp, error));
    }
    debug!(path = %path.display(), "wrote bundle");
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read the size of {}", path.display()))?;
    Ok(metadata.len())
}

/// Return the bundle a new one would duplicate. For a file output that is the file itself, and
/// for a directory output it's the newest bundle of this repo.
pub fn previous_bundle(output: &Output, repo: &RepoName) -> Result<Option<PathBuf>> {
    match output {
        Output::File(path) => Ok(path.is_file().then(|| path.clone())),
        Output::Directory(dir) => {
            let newest = dir.find(repo)?.into_iter().next();
            Ok(newest.map(|bundle| bundle.path))
        }
    }
}

/// How a bundle's refs compare with the refs a new bundle would hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefMatch {
    Current,
    OutOfDate,
    /// The bundle's refs can't be read, so it can't be reused and a new bundle is made instead.
    Unreadable,
}

/// Compare the refs recorded in `bundle` with `current`, the refs a new bundle would hold. Take
/// them read once by the caller, so comparing many bundles reads the repo only once.
pub fn compare_refs(git: &Git, bundle: &Path, current: &BTreeSet<RefTip>) -> RefMatch {
    let recorded = match git.bundle_refs(bundle) {
        Ok(recorded) => recorded,
        Err(error) => {
            debug!("can't read the refs of {}: {error:#}", bundle.display());
            return RefMatch::Unreadable;
        }
    };
    if &recorded == current {
        return RefMatch::Current;
    }
    RefMatch::OutOfDate
}

/// Refuse a name the template's matcher can't read back, since pruning and the skip check
/// would never see that bundle.
fn render_name(template: &NameTemplate, values: &NameValues) -> Result<String> {
    let name = template.render(values);
    let matcher = bundles::matcher(template, values.repo)?;
    if matcher.parse(&name).is_none() {
        bail!("bundle name `{name}` does not match its own template `{template}`");
    }
    Ok(name)
}

/// Keep the temporary name from ending in `.bundle`, so no template's matcher ever picks it up.
fn temp_path(path: &Path) -> Result<PathBuf> {
    let mut name = path
        .file_name()
        .with_context(|| format!("bundle path {} has no file name", path.display()))?
        .to_os_string();
    name.push(TEMP_SUFFIX);
    Ok(path.with_file_name(name))
}

/// Give a directory this creates inside the repo a `.gitignore` that ignores everything in it,
/// so git never lists the bundles as untracked. Leave an existing directory alone, since it may
/// hold files the user wants tracked.
fn create_parent_dir(git: &Git, path: &Path) -> Result<()> {
    let dir = path
        .parent()
        .with_context(|| format!("bundle path {} has no parent directory", path.display()))?;
    if !create_dir(dir)? || git.path_in_repo(dir)?.is_none() {
        return Ok(());
    }
    let ignore_file = dir.join(IGNORE_FILE);
    fs::write(&ignore_file, IGNORE_ALL)
        .with_context(|| format!("failed to write {}", ignore_file.display()))
}

/// Create `dir` and any missing parents. Return whether `dir` itself was new.
fn create_dir(dir: &Path) -> Result<bool> {
    let context = || format!("failed to create output directory {}", dir.display());
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).with_context(context)?;
    }
    match fs::create_dir(dir) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error).with_context(context),
    }
}

fn create_and_verify(git: &Git, bundle: &Path, refs: &RefSet) -> Result<()> {
    git.create_bundle(bundle, refs)?;
    git.verify_bundle(bundle)
}

fn move_into_place(temp: &Path, path: &Path) -> Result<()> {
    fs::rename(temp, path)
        .with_context(|| format!("failed to move {} to {}", temp.display(), path.display()))
}

/// Delete the temporary file and return `error`, noting a failed delete instead of hiding it.
fn discard(temp: &Path, error: anyhow::Error) -> anyhow::Error {
    match fs::remove_file(temp) {
        Ok(()) => error,
        Err(remove_error) if remove_error.kind() == io::ErrorKind::NotFound => error,
        Err(remove_error) => error.context(format!(
            "failed to delete partial bundle {} ({remove_error})",
            temp.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundles::BundleDir;
    use crate::git::RefSelection;
    use crate::naming::DEFAULT_TEMPLATE;
    use crate::test_support::{empty_repo, run_with_identity, test_repo};
    use chrono::TimeZone;
    use tempfile::TempDir;

    const REPO: &str = "myrepo";

    fn repo_name() -> RepoName {
        RepoName::try_from(REPO.to_string()).unwrap()
    }

    fn default_template() -> NameTemplate {
        DEFAULT_TEMPLATE.parse().unwrap()
    }

    fn refs_of(git: &Git, selection: RefSelection) -> RefSet {
        git.ref_set(&selection.into()).unwrap()
    }

    fn all_refs(git: &Git) -> RefSet {
        refs_of(git, RefSelection::All)
    }

    fn compare(git: &Git, bundle: &Path, refs: &RefSet) -> RefMatch {
        compare_refs(git, bundle, &git.current_refs(refs).unwrap())
    }

    fn directory(path: PathBuf, name: NameTemplate) -> Output {
        Output::Directory(BundleDir { path, name })
    }

    // Bundle path

    #[test]
    fn file_output_is_used_as_is() {
        let (dir, git) = test_repo();
        let file = dir.path().join("snapshot.bundle");
        let output = Output::File(file.clone());
        let path = bundle_path(&git, &output, &repo_name(), &all_refs(&git)).unwrap();
        assert_eq!(path, file);
    }

    #[test]
    fn directory_output_renders_a_name_that_parses_back() {
        let (dir, git) = test_repo();
        let out = dir.path().join("bundles");
        let output = directory(out.clone(), default_template());

        let path = bundle_path(&git, &output, &repo_name(), &all_refs(&git)).unwrap();
        assert_eq!(path.parent(), Some(out.as_path()));
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let matcher = default_template().matcher(REPO).unwrap();
        assert!(matcher.parse(file_name).is_some(), "{file_name}");
    }

    #[test]
    fn branch_in_name_comes_from_the_ref_set() {
        let (dir, git) = test_repo();
        let template = "{branch}@{repo}.bundle".parse().unwrap();
        let output = directory(dir.path().to_path_buf(), template);

        let path = bundle_path(&git, &output, &repo_name(), &all_refs(&git)).unwrap();
        assert_eq!(path.file_name().unwrap(), "main@myrepo.bundle");
    }

    #[test]
    fn repo_without_commits_has_no_bundle_path() {
        let (dir, git) = empty_repo();
        let output = Output::File(dir.path().join("x.bundle"));
        let error = bundle_path(&git, &output, &repo_name(), &all_refs(&git)).unwrap_err();
        assert!(error.to_string().contains("has no commits"), "{error}");
    }

    #[test]
    fn rejects_name_that_would_not_parse_back() {
        let values = NameValues {
            created_at: Utc.with_ymd_and_hms(2026, 10, 4, 15, 30, 12).unwrap(),
            hash: "abc1",
            repo: REPO,
            branch: Some("main"),
        };
        let error = render_name(&default_template(), &values).unwrap_err();
        assert!(error.to_string().contains("abc1"), "{error}");
    }

    // Writing

    #[test]
    fn writes_verified_bundle_into_new_directory() {
        let (dir, git) = test_repo();
        let path = dir.path().join("a/b/repo.bundle");

        let size = write_bundle(&git, &path, &all_refs(&git)).unwrap();
        assert_eq!(size, fs::metadata(&path).unwrap().len());
        assert!(git.verify_bundle(&path).is_ok());
        assert!(!temp_path(&path).unwrap().exists());
    }

    #[test]
    fn new_directory_inside_the_repo_ignores_itself() {
        let (dir, git) = test_repo();
        let path = dir.path().join("a/bundles/repo.bundle");

        write_bundle(&git, &path, &all_refs(&git)).unwrap();
        assert!(dir.path().join("a/bundles/.gitignore").is_file());
        assert!(!dir.path().join("a/.gitignore").exists());
        assert!(git.status_lines(None).unwrap().is_empty());
    }

    #[test]
    fn existing_directory_gets_no_ignore_file() {
        let (dir, git) = test_repo();
        fs::create_dir(dir.path().join("bundles")).unwrap();
        let path = dir.path().join("bundles/repo.bundle");

        write_bundle(&git, &path, &all_refs(&git)).unwrap();
        assert!(!dir.path().join("bundles/.gitignore").exists());
    }

    #[test]
    fn new_directory_outside_the_repo_gets_no_ignore_file() {
        let (_dir, git) = test_repo();
        let outside = TempDir::new().unwrap();
        let path = outside.path().join("bundles/repo.bundle");

        write_bundle(&git, &path, &all_refs(&git)).unwrap();
        assert!(!outside.path().join("bundles/.gitignore").exists());
    }

    #[test]
    fn replaces_existing_file() {
        let (dir, git) = test_repo();
        let path = dir.path().join("repo.bundle");
        fs::write(&path, "old").unwrap();

        write_bundle(&git, &path, &all_refs(&git)).unwrap();
        assert!(git.verify_bundle(&path).is_ok());
    }

    #[test]
    fn failed_write_keeps_existing_file() {
        let (dir, git) = empty_repo();
        let path = dir.path().join("repo.bundle");
        fs::write(&path, "old").unwrap();

        assert!(write_bundle(&git, &path, &all_refs(&git)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert!(!temp_path(&path).unwrap().exists());
    }

    // Previous bundle

    #[test]
    fn file_output_is_its_own_previous_bundle_once_it_exists() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("snapshot.bundle");
        let output = Output::File(file.clone());
        assert_eq!(previous_bundle(&output, &repo_name()).unwrap(), None);

        fs::write(&file, "").unwrap();
        assert_eq!(previous_bundle(&output, &repo_name()).unwrap(), Some(file));
    }

    #[test]
    fn directory_output_compares_with_the_newest_bundle() {
        let dir = TempDir::new().unwrap();
        let older = dir.path().join("20261004T153012Z-a1b2c3d-myrepo.bundle");
        let newer = dir.path().join("20261005T090000Z-b2c3d4e-myrepo.bundle");
        fs::write(&older, "").unwrap();
        fs::write(&newer, "").unwrap();
        let output = directory(dir.path().to_path_buf(), default_template());

        assert_eq!(previous_bundle(&output, &repo_name()).unwrap(), Some(newer));
    }

    #[test]
    fn missing_output_directory_has_no_previous_bundle() {
        let dir = TempDir::new().unwrap();
        let output = directory(dir.path().join("bundles"), default_template());
        assert_eq!(previous_bundle(&output, &repo_name()).unwrap(), None);
    }

    // Current refs

    #[test]
    fn fresh_bundle_has_current_refs() {
        let (dir, git) = test_repo();
        let bundle = dir.path().join("repo.bundle");
        let refs = all_refs(&git);
        write_bundle(&git, &bundle, &refs).unwrap();

        assert_eq!(compare(&git, &bundle, &refs), RefMatch::Current);
    }

    #[test]
    fn new_commit_makes_bundle_out_of_date() {
        let (dir, git) = test_repo();
        let bundle = dir.path().join("repo.bundle");
        let refs = all_refs(&git);
        write_bundle(&git, &bundle, &refs).unwrap();
        run_with_identity(&git, &["commit", "-q", "--allow-empty", "-m", "second"]);

        assert_eq!(compare(&git, &bundle, &refs), RefMatch::OutOfDate);
    }

    #[test]
    fn moving_another_branch_matters_only_when_it_is_bundled() {
        let (dir, git) = test_repo();
        git.run(["branch", "feature"]).unwrap();
        run_with_identity(&git, &["commit", "-q", "--allow-empty", "-m", "second"]);
        let every_ref = all_refs(&git);
        let head_only = refs_of(&git, RefSelection::Head);
        let all = dir.path().join("all.bundle");
        let head = dir.path().join("head.bundle");
        write_bundle(&git, &all, &every_ref).unwrap();
        write_bundle(&git, &head, &head_only).unwrap();

        git.run(["branch", "-f", "feature", "main"]).unwrap();
        assert_eq!(compare(&git, &all, &every_ref), RefMatch::OutOfDate);
        assert_eq!(compare(&git, &head, &head_only), RefMatch::Current);
    }

    #[test]
    fn unreadable_bundle_is_reported_as_unreadable() {
        let (dir, git) = test_repo();
        let bundle = dir.path().join("repo.bundle");
        fs::write(&bundle, "not a bundle").unwrap();

        assert_eq!(
            compare(&git, &bundle, &all_refs(&git)),
            RefMatch::Unreadable
        );
    }
}
