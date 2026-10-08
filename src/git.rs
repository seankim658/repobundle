use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use serde::de::{self, IntoDeserializer, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use tracing::debug;

const GIT: &str = "git";
const HEAD: &str = "HEAD";
const REF_FORMAT: &str = "--format=%(objectname) %(refname)";

/// Fail with a clear message when the `git` binary can't be run.
pub fn ensure_installed() -> Result<()> {
    let output = match Command::new(GIT).arg("--version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            bail!("git was not found on the PATH; install git and try again")
        }
        Err(error) => return Err(error).context("failed to run `git --version`"),
    };
    if !output.status.success() {
        bail!("`git --version` failed ({})", output.status);
    }
    let version = String::from_utf8_lossy(&output.stdout);
    debug!(version = %version.trim(), "found git");
    Ok(())
}

/// Which ref tips go into a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefSelection {
    All,
    // The default, since remote-tracking refs move on every fetch and would make the last
    // bundle look out of date without any new commits. A doc comment here would show up as
    // help for this value in `--help`.
    #[default]
    Branches,
    Head,
}

/// The refs a run asks for, before git has checked them. `--refs` only picks a selection, while
/// config can also list refs by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefSpec {
    Selection(RefSelection),
    /// Ref names as written in config, such as `main` or `refs/tags/v1`.
    Named(Vec<String>),
}

impl Default for RefSpec {
    fn default() -> Self {
        Self::Selection(RefSelection::default())
    }
}

impl From<RefSelection> for RefSpec {
    fn from(selection: RefSelection) -> Self {
        Self::Selection(selection)
    }
}

/// Read either a selection name or a list of ref names, so `refs` in config takes both forms.
impl<'de> Deserialize<'de> for RefSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(RefSpecVisitor)
    }
}

struct RefSpecVisitor;

impl<'de> Visitor<'de> for RefSpecVisitor {
    type Value = RefSpec;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("\"all\", \"branches\", \"head\", or a list of ref names")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<RefSpec, E> {
        let deserializer: de::value::StrDeserializer<'_, E> = value.into_deserializer();
        RefSelection::deserialize(deserializer).map(RefSpec::Selection)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<RefSpec, A::Error> {
        let mut names = Vec::new();
        while let Some(name) = seq.next_element::<String>()? {
            check_ref_name(&name).map_err(<A::Error as de::Error>::custom)?;
            names.push(name);
        }
        if names.is_empty() {
            return Err(de::Error::custom(
                "the `refs` list is empty; list at least one ref, or use \"head\"",
            ));
        }
        Ok(RefSpec::Named(names))
    }
}

/// Refuse a name git would read as an option, since it's passed to git as an argument.
fn check_ref_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a ref name in `refs` is empty".to_string());
    }
    if name.starts_with('-') {
        return Err(format!(
            "ref name `{name}` in `refs` must not start with `-`"
        ));
    }
    Ok(())
}

fn branch_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// The refs to bundle, tied to the branch that was checked out when they were chosen. Build
/// both the bundle and the skip check from one value, so they always agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSet {
    scope: RefScope,
    /// The checked-out branch's short name, or `None` on a detached HEAD.
    branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RefScope {
    Selection(RefSelection),
    /// Full ref names, each checked to exist.
    Named(BTreeSet<String>),
}

impl RefSet {
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    fn bundle_args(&self) -> Vec<String> {
        match &self.scope {
            RefScope::Selection(selection) => self.selection_args(*selection),
            RefScope::Named(names) => {
                let mut args = vec![HEAD.to_string()];
                args.extend(names.iter().cloned());
                args
            }
        }
    }

    /// Return the `for-each-ref` patterns that list the same refs as `bundle_args`, minus `HEAD`.
    /// A full ref name that exists matches only itself, since git forbids a ref with refs below
    /// it.
    fn ref_patterns(&self) -> Vec<String> {
        match &self.scope {
            RefScope::Selection(selection) => self.selection_patterns(*selection),
            RefScope::Named(names) => names.iter().cloned().collect(),
        }
    }

    fn selection_args(&self, selection: RefSelection) -> Vec<String> {
        match selection {
            RefSelection::All => vec!["--all".to_string()],
            RefSelection::Branches => vec![
                "--branches".to_string(),
                "--tags".to_string(),
                HEAD.to_string(),
            ],
            RefSelection::Head => {
                let mut args = vec![HEAD.to_string()];
                args.extend(self.branch().map(branch_ref));
                args
            }
        }
    }

    fn selection_patterns(&self, selection: RefSelection) -> Vec<String> {
        match selection {
            RefSelection::All => vec!["refs".to_string()],
            RefSelection::Branches => vec!["refs/heads".to_string(), "refs/tags".to_string()],
            RefSelection::Head => self.branch().map(branch_ref).into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RefTip {
    pub name: String,
    pub object_id: String,
}

/// Runs git commands against one repository. Bundle paths must be absolute, because git runs
/// inside the repo directory and would resolve a relative path against it.
#[derive(Debug, Clone)]
pub struct Git {
    repo_dir: PathBuf,
}

impl Git {
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
        }
    }

    /// Find the root of the repository containing `path`, so running from a subdirectory works.
    pub fn discover(path: &Path) -> Result<Self> {
        let root = Self::new(path)
            .run(["rev-parse", "--show-toplevel"])
            .with_context(|| {
                format!("the path {} is not inside a git repository", path.display())
            })?;
        Ok(Self::new(root))
    }

    pub fn repo_dir(&self) -> &Path {
        &self.repo_dir
    }

    /// Run git in the repo directory and return its stdout without trailing whitespace.
    pub fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let (command_line, output) = self.execute(args)?;
        if !output.status.success() {
            return Err(command_error(&command_line, &output));
        }
        let stdout = String::from_utf8(output.stdout)
            .with_context(|| format!("`git {command_line}` printed non-UTF-8 output"))?;
        Ok(stdout.trim_end().to_string())
    }

    pub fn ensure_has_commits(&self) -> Result<()> {
        if self.run_check(["rev-parse", "--verify", "--quiet", HEAD])? {
            return Ok(());
        }
        bail!(
            "the repository at {} has no commits yet; make a commit first",
            self.repo_dir.display()
        )
    }

    /// Refuse a shallow clone. Its bundle passes `git bundle verify`, but cloning from it fails
    /// because the missing parent commits are never sent.
    pub fn ensure_not_shallow(&self) -> Result<()> {
        if !self.is_shallow()? {
            return Ok(());
        }
        bail!(
            "the repository at {} is a shallow clone, so a bundle of it could not be cloned; run `git fetch --unshallow` first",
            self.repo_dir.display()
        )
    }

    pub fn short_hash(&self) -> Result<String> {
        self.run(["rev-parse", "--short=7", HEAD])
    }

    /// Read the checked-out branch once, so every use of the returned refs sees the same one.
    /// Check each named ref and turn it into its full name.
    pub fn ref_set(&self, spec: &RefSpec) -> Result<RefSet> {
        let scope = match spec {
            RefSpec::Selection(selection) => RefScope::Selection(*selection),
            RefSpec::Named(names) => RefScope::Named(self.full_ref_names(names)?),
        };
        Ok(RefSet {
            scope,
            branch: self.current_branch()?,
        })
    }

    /// Skip `HEAD`, which every bundle holds anyway, so it never turns into the branch it points
    /// to.
    fn full_ref_names(&self, names: &[String]) -> Result<BTreeSet<String>> {
        names
            .iter()
            .filter(|name| name.as_str() != HEAD)
            .map(|name| self.full_ref_name(name))
            .collect()
    }

    /// Return the full name of the ref `name` resolves to, such as `refs/heads/main` for `main`.
    /// The bundle records full names, so the skip check would never match a short one. Refuse a
    /// name that isn't exactly one ref, such as a commit hash, since a bundle can only carry refs.
    fn full_ref_name(&self, name: &str) -> Result<String> {
        let args = [
            "rev-parse",
            "--verify",
            "--quiet",
            "--symbolic-full-name",
            name,
        ];
        let (command_line, output) = self.execute(args)?;
        match output.status.code() {
            Some(0) => {}
            Some(1) => bail!("the ref `{name}` listed in `refs` does not exist"),
            _ => return Err(command_error(&command_line, &output)),
        }
        let full_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if full_name.is_empty() {
            bail!(
                "`{name}` listed in `refs` is not a single ref; use a branch, tag, or full ref name such as `refs/tags/v1`"
            );
        }
        Ok(full_name)
    }

    /// Return the checked-out branch's short name, or `None` on a detached HEAD.
    fn current_branch(&self) -> Result<Option<String>> {
        let branch = self.run(["branch", "--show-current"])?;
        if branch.is_empty() {
            return Ok(None);
        }
        Ok(Some(branch))
    }

    /// Return one `git status --porcelain` line per changed, staged, or untracked path, leaving
    /// out `exclude`, which is relative to the repo root.
    pub fn status_lines(&self, exclude: Option<&Path>) -> Result<Vec<String>> {
        let mut args: Vec<OsString> = vec!["status".into(), "--porcelain".into()];
        if let Some(path) = exclude {
            let mut pathspec = OsString::from(":(exclude,literal)");
            pathspec.push(path);
            args.extend(["--".into(), pathspec]);
        }
        let output = self.run(&args)?;
        Ok(output.lines().map(str::to_string).collect())
    }

    fn is_shallow(&self) -> Result<bool> {
        let output = self.run(["rev-parse", "--is-shallow-repository"])?;
        Ok(output == "true")
    }

    /// Report whether git ignores `path`, which is relative to the repo root.
    pub fn is_ignored(&self, path: &Path) -> Result<bool> {
        self.run_check([
            OsStr::new("check-ignore"),
            OsStr::new("-q"),
            path.as_os_str(),
        ])
    }

    /// Return `path` relative to the repo root when it exists strictly inside the repo. Resolve
    /// both paths first, so `..` and symlinks can't make an outside path look inside.
    pub fn path_in_repo(&self, path: &Path) -> Result<Option<PathBuf>> {
        if !path.exists() {
            return Ok(None);
        }
        let target = canonicalize(path)?;
        let root = canonicalize(&self.repo_dir)?;
        match target.strip_prefix(&root) {
            Ok(relative) if !relative.as_os_str().is_empty() => Ok(Some(relative.to_path_buf())),
            _ => Ok(None),
        }
    }

    pub fn create_bundle(&self, bundle: &Path, refs: &RefSet) -> Result<()> {
        assert_absolute(bundle);
        let mut args: Vec<OsString> = vec!["bundle".into(), "create".into(), bundle.into()];
        args.extend(refs.bundle_args().into_iter().map(OsString::from));
        self.run(&args)?;
        Ok(())
    }

    pub fn verify_bundle(&self, bundle: &Path) -> Result<()> {
        assert_absolute(bundle);
        self.run([
            OsStr::new("bundle"),
            OsStr::new("verify"),
            bundle.as_os_str(),
        ])?;
        Ok(())
    }

    /// Return the refs a new bundle of `refs` would contain, including `HEAD`.
    pub fn current_refs(&self, refs: &RefSet) -> Result<BTreeSet<RefTip>> {
        let mut tips = self.refs_matching(&refs.ref_patterns())?;
        tips.insert(RefTip {
            name: HEAD.to_string(),
            object_id: self.run(["rev-parse", HEAD])?,
        });
        Ok(tips)
    }

    /// Return the refs recorded in a bundle file.
    pub fn bundle_refs(&self, bundle: &Path) -> Result<BTreeSet<RefTip>> {
        assert_absolute(bundle);
        let output = self.run([
            OsStr::new("bundle"),
            OsStr::new("list-heads"),
            bundle.as_os_str(),
        ])?;
        parse_ref_lines(&output)
    }

    /// Run a command whose exit code is the answer: 0 means yes, 1 means no, anything else is an error.
    fn run_check<I, S>(&self, args: I) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let (command_line, output) = self.execute(args)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_error(&command_line, &output)),
        }
    }

    /// Start git, wait for it, and log the command and its exit status.
    fn execute<I, S>(&self, args: I) -> Result<(String, Output)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args.into_iter().map(|arg| arg.as_ref().into()).collect();
        let command_line = describe(&args);
        debug!(repo = %self.repo_dir.display(), command = %command_line, "running git");
        let output = Command::new(GIT)
            .arg("-C")
            .arg(&self.repo_dir)
            .args(&args)
            .output()
            .with_context(|| format!("failed to run `git {command_line}`"))?;
        debug!(command = %command_line, status = %output.status, "git finished");
        Ok((command_line, output))
    }

    /// Return no refs for no patterns, because `for-each-ref` with no patterns lists every ref.
    fn refs_matching(&self, patterns: &[String]) -> Result<BTreeSet<RefTip>> {
        if patterns.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut args = vec!["for-each-ref".to_string(), REF_FORMAT.to_string()];
        args.extend(patterns.iter().cloned());
        parse_ref_lines(&self.run(&args)?)
    }
}

fn describe(args: &[OsString]) -> String {
    let parts: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
    parts.join(" ")
}

fn canonicalize(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))
}

/// Catch relative bundle paths as a bug in the caller.
fn assert_absolute(bundle: &Path) {
    assert!(
        bundle.is_absolute(),
        "bundle path {} must be absolute",
        bundle.display()
    );
}

fn command_error(command_line: &str, output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow!(
        "`git {command_line}` failed ({}): {}",
        output.status,
        stderr.trim()
    )
}

fn parse_ref_lines(output: &str) -> Result<BTreeSet<RefTip>> {
    output.lines().map(parse_ref_line).collect()
}

/// Parse one `<object id> <ref name>` line, the format shared by `for-each-ref` and `bundle list-heads`.
fn parse_ref_line(line: &str) -> Result<RefTip> {
    let (object_id, name) = line
        .split_once(' ')
        .with_context(|| format!("unexpected ref line from git: {line:?}"))?;
    Ok(RefTip {
        name: name.to_string(),
        object_id: object_id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{empty_repo, run_with_identity, test_repo};
    use tempfile::TempDir;

    fn repo_with_every_ref_kind() -> (TempDir, Git) {
        let (_upstream_dir, upstream) = test_repo();
        let dir = TempDir::new().unwrap();
        upstream
            .run([
                OsStr::new("clone"),
                OsStr::new("-q"),
                OsStr::new("."),
                dir.path().as_os_str(),
            ])
            .unwrap();
        let git = Git::new(dir.path());

        git.run(["branch", "feature"]).unwrap();
        run_with_identity(&git, &["tag", "-a", "v1", "-m", "v1"]);
        run_with_identity(&git, &["notes", "add", "-m", "note"]);
        fs::write(dir.path().join("wip.txt"), "wip").unwrap();
        git.run(["add", "wip.txt"]).unwrap();
        run_with_identity(&git, &["stash", "push", "-q"]);
        (dir, git)
    }

    fn fixed_refs(selection: RefSelection, branch: Option<&str>) -> RefSet {
        RefSet {
            scope: RefScope::Selection(selection),
            branch: branch.map(str::to_string),
        }
    }

    fn refs_of(git: &Git, selection: RefSelection) -> RefSet {
        git.ref_set(&selection.into()).unwrap()
    }

    fn named(names: &[&str]) -> RefSpec {
        RefSpec::Named(names.iter().map(|name| name.to_string()).collect())
    }

    fn ref_names(tips: &BTreeSet<RefTip>) -> Vec<&str> {
        tips.iter().map(|tip| tip.name.as_str()).collect()
    }

    fn parse_spec(toml_value: &str) -> Result<RefSpec, toml::de::Error> {
        #[derive(Debug, Deserialize)]
        struct Wrapper {
            refs: RefSpec,
        }
        let wrapper: Wrapper = toml::from_str(&format!("refs = {toml_value}"))?;
        Ok(wrapper.refs)
    }

    // RefSpec

    #[test]
    fn spec_reads_a_selection_name() {
        let spec = parse_spec("\"head\"").unwrap();
        assert_eq!(spec, RefSpec::Selection(RefSelection::Head));
    }

    #[test]
    fn spec_reads_a_list_of_names() {
        let spec = parse_spec("[\"main\", \"refs/tags/v1\"]").unwrap();
        assert_eq!(spec, named(&["main", "refs/tags/v1"]));
    }

    #[test]
    fn spec_rejects_unknown_selection_and_wrong_types() {
        for value in ["\"everything\"", "3"] {
            let error = parse_spec(value).unwrap_err().to_string();
            assert!(error.contains("branches"), "{value}: {error}");
        }
    }

    #[test]
    fn spec_rejects_empty_lists_and_names() {
        for value in ["[]", "[\"\"]"] {
            let error = parse_spec(value).unwrap_err().to_string();
            assert!(error.contains("empty"), "{value}: {error}");
        }
    }

    #[test]
    fn spec_rejects_names_that_look_like_options() {
        let error = parse_spec("[\"--all\"]").unwrap_err().to_string();
        assert!(error.contains("must not start with `-`"), "{error}");
    }

    // RefSet

    #[test]
    fn all_selection_bundles_every_ref() {
        let refs = fixed_refs(RefSelection::All, Some("main"));
        assert_eq!(refs.bundle_args(), ["--all"]);
    }

    #[test]
    fn branches_selection_bundles_branches_tags_and_head() {
        let refs = fixed_refs(RefSelection::Branches, Some("main"));
        assert_eq!(refs.bundle_args(), ["--branches", "--tags", "HEAD"]);
    }

    #[test]
    fn head_selection_bundles_head_and_current_branch() {
        let refs = fixed_refs(RefSelection::Head, Some("main"));
        assert_eq!(refs.bundle_args(), ["HEAD", "refs/heads/main"]);
    }

    #[test]
    fn head_selection_on_detached_head_bundles_only_head() {
        let refs = fixed_refs(RefSelection::Head, None);
        assert_eq!(refs.bundle_args(), ["HEAD"]);
    }

    // Running git

    #[test]
    fn finds_git_on_path() {
        assert!(ensure_installed().is_ok());
    }

    #[test]
    fn run_returns_trimmed_stdout() {
        let (_dir, git) = test_repo();
        assert_eq!(git.run(["branch", "--show-current"]).unwrap(), "main");
    }

    #[test]
    fn run_error_includes_git_stderr() {
        let (_dir, git) = test_repo();
        let error = git.run(["rev-parse", "missing-branch"]).unwrap_err();
        assert!(error.to_string().contains("unknown revision"), "{error}");
    }

    // Repo queries

    #[test]
    fn discovers_repo_root_from_subdirectory() {
        let (dir, _git) = test_repo();
        let subdirectory = dir.path().join("nested");
        fs::create_dir(&subdirectory).unwrap();

        let git = Git::discover(&subdirectory).unwrap();
        assert_eq!(
            fs::canonicalize(git.repo_dir()).unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn discover_outside_repo_explains_the_problem() {
        let dir = TempDir::new().unwrap();
        let error = Git::discover(dir.path()).unwrap_err();
        assert!(
            error.to_string().contains("is not inside a git repository"),
            "{error}"
        );
    }

    #[test]
    fn repo_without_commits_is_rejected() {
        let (_dir, git) = empty_repo();
        let error = git.ensure_has_commits().unwrap_err();
        assert!(error.to_string().contains("has no commits"), "{error}");
    }

    #[test]
    fn repo_with_commits_is_accepted() {
        let (_dir, git) = test_repo();
        assert!(git.ensure_has_commits().is_ok());
    }

    #[test]
    fn short_hash_is_prefix_of_head() {
        let (_dir, git) = test_repo();
        let short = git.short_hash().unwrap();
        let full = git.run(["rev-parse", "HEAD"]).unwrap();
        assert!(short.len() >= 7 && full.starts_with(&short), "{short}");
    }

    #[test]
    fn short_hash_ignores_short_core_abbrev() {
        let (_dir, git) = test_repo();
        git.run(["config", "core.abbrev", "4"]).unwrap();
        let short = git.short_hash().unwrap();
        assert!(short.len() >= 7, "{short}");
    }

    #[test]
    fn current_branch_is_none_on_detached_head() {
        let (_dir, git) = test_repo();
        assert_eq!(git.current_branch().unwrap().as_deref(), Some("main"));

        git.run(["checkout", "-q", "--detach"]).unwrap();
        assert_eq!(git.current_branch().unwrap(), None);
    }

    #[test]
    fn status_lines_list_staged_and_untracked_paths() {
        let (dir, git) = test_repo();
        assert!(git.status_lines(None).unwrap().is_empty());

        fs::write(dir.path().join("new.txt"), "new").unwrap();
        fs::write(dir.path().join("staged.txt"), "staged").unwrap();
        git.run(["add", "staged.txt"]).unwrap();
        let lines = git.status_lines(None).unwrap();
        assert!(lines.contains(&"?? new.txt".to_string()), "{lines:?}");
        assert!(lines.contains(&"A  staged.txt".to_string()), "{lines:?}");
    }

    #[test]
    fn status_lines_leave_out_the_excluded_path() {
        let (dir, git) = test_repo();
        fs::create_dir(dir.path().join("bundles")).unwrap();
        fs::write(dir.path().join("bundles/a.bundle"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        let lines = git.status_lines(Some(Path::new("bundles"))).unwrap();
        assert_eq!(lines, ["?? notes.txt"]);
    }

    #[test]
    fn is_ignored_follows_gitignore() {
        let (dir, git) = test_repo();
        fs::write(dir.path().join(".gitignore"), "/bundles/\n").unwrap();
        fs::create_dir(dir.path().join("bundles")).unwrap();
        fs::create_dir(dir.path().join("other")).unwrap();

        assert!(git.is_ignored(Path::new("bundles")).unwrap());
        assert!(!git.is_ignored(Path::new("other")).unwrap());
    }

    #[test]
    fn shallow_clone_is_rejected() {
        let (upstream_dir, upstream) = test_repo();
        run_with_identity(
            &upstream,
            &["commit", "-q", "--allow-empty", "-m", "second"],
        );
        let dir = TempDir::new().unwrap();
        let url = format!("file://{}", upstream_dir.path().display());
        let target = dir.path().to_str().unwrap();
        upstream
            .run(["clone", "-q", "--depth", "1", url.as_str(), target])
            .unwrap();

        assert!(upstream.ensure_not_shallow().is_ok());
        let error = Git::new(dir.path()).ensure_not_shallow().unwrap_err();
        assert!(error.to_string().contains("--unshallow"), "{error}");
    }

    // Bundles

    #[test]
    fn created_bundle_verifies() {
        let (dir, git) = test_repo();
        let bundle = dir.path().join("repo.bundle");
        git.create_bundle(&bundle, &refs_of(&git, RefSelection::All))
            .unwrap();
        assert!(git.verify_bundle(&bundle).is_ok());
    }

    #[test]
    fn corrupt_bundle_fails_verification() {
        let (dir, git) = test_repo();
        let bundle = dir.path().join("corrupt.bundle");
        fs::write(&bundle, "not a bundle").unwrap();
        assert!(git.verify_bundle(&bundle).is_err());
    }

    #[test]
    #[should_panic(expected = "must be absolute")]
    fn relative_bundle_path_panics() {
        let (_dir, git) = test_repo();
        git.verify_bundle(Path::new("repo.bundle")).unwrap();
    }

    #[test]
    fn bundle_refs_match_current_refs_for_every_selection() {
        let (dir, git) = repo_with_every_ref_kind();

        for selection in [
            RefSelection::All,
            RefSelection::Branches,
            RefSelection::Head,
        ] {
            let bundle = dir.path().join(format!("{selection:?}.bundle"));
            let refs = refs_of(&git, selection);
            git.create_bundle(&bundle, &refs).unwrap();
            assert_eq!(
                git.bundle_refs(&bundle).unwrap(),
                git.current_refs(&refs).unwrap(),
                "{selection:?}"
            );
        }
    }

    #[test]
    fn all_selection_reads_every_ref_kind() {
        let (_dir, git) = repo_with_every_ref_kind();
        let refs = git.current_refs(&refs_of(&git, RefSelection::All)).unwrap();
        let names: BTreeSet<&str> = refs.iter().map(|tip| tip.name.as_str()).collect();

        for expected in [
            "HEAD",
            "refs/heads/main",
            "refs/heads/feature",
            "refs/tags/v1",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            "refs/notes/commits",
            "refs/stash",
        ] {
            assert!(
                names.contains(expected),
                "{expected} missing from {names:?}"
            );
        }
    }

    #[test]
    fn head_selection_on_detached_head_reads_only_head() {
        let (_dir, git) = test_repo();
        git.run(["checkout", "-q", "--detach"]).unwrap();
        let refs = git
            .current_refs(&refs_of(&git, RefSelection::Head))
            .unwrap();
        assert_eq!(ref_names(&refs), ["HEAD"]);
    }

    // Named refs

    #[test]
    fn named_refs_resolve_to_full_names() {
        let (_dir, git) = repo_with_every_ref_kind();
        let spec = named(&["feature", "v1", "origin/main", "refs/heads/main"]);
        let refs = git.current_refs(&git.ref_set(&spec).unwrap()).unwrap();

        let expected = [
            "HEAD",
            "refs/heads/feature",
            "refs/heads/main",
            "refs/remotes/origin/main",
            "refs/tags/v1",
        ];
        assert_eq!(ref_names(&refs), expected);
    }

    #[test]
    fn named_head_stays_head() {
        let (_dir, git) = test_repo();
        let refs = git.ref_set(&named(&["HEAD"])).unwrap();
        assert_eq!(ref_names(&git.current_refs(&refs).unwrap()), ["HEAD"]);
    }

    #[test]
    fn named_bundle_matches_its_current_refs() {
        let (dir, git) = repo_with_every_ref_kind();
        let refs = git.ref_set(&named(&["feature", "v1"])).unwrap();
        let bundle = dir.path().join("named.bundle");
        git.create_bundle(&bundle, &refs).unwrap();

        assert_eq!(
            git.bundle_refs(&bundle).unwrap(),
            git.current_refs(&refs).unwrap()
        );
    }

    #[test]
    fn missing_named_ref_is_rejected() {
        let (_dir, git) = test_repo();
        let error = git.ref_set(&named(&["nope"])).unwrap_err().to_string();
        assert!(
            error.contains("`nope`") && error.contains("does not exist"),
            "{error}"
        );
    }

    #[test]
    fn commit_hash_is_not_a_named_ref() {
        let (_dir, git) = test_repo();
        let hash = git.run(["rev-parse", "HEAD"]).unwrap();
        let error = git.ref_set(&named(&[&hash])).unwrap_err().to_string();
        assert!(error.contains("not a single ref"), "{error}");
    }

    #[test]
    fn ambiguous_named_ref_is_rejected() {
        let (_dir, git) = test_repo();
        git.run(["branch", "both"]).unwrap();
        git.run(["tag", "both"]).unwrap();
        let error = git.ref_set(&named(&["both"])).unwrap_err().to_string();
        assert!(error.contains("not a single ref"), "{error}");
    }
}
