use std::fmt;
use std::fs;
use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::Git;
use crate::output::{count_bundles, display_path};
use crate::settings::Output;
use crate::size::{BYTES_PER_MB, format_size};

const SUBMODULES_FILE: &str = ".gitmodules";
const ATTRIBUTES_FILE: &str = ".gitattributes";
const LFS_ATTRIBUTE: &str = "filter=lfs";
const MAX_LISTED_CHANGES: usize = 10;
/// How `git status --porcelain` starts the line for an untracked path.
const UNTRACKED_PREFIX: &str = "??";

/// Something the user should know before relying on a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Holds the `git status --porcelain` lines for the paths the bundle leaves out.
    UncommittedChanges(Vec<String>),
    /// Holds the `git status --porcelain` lines for untracked paths, which `--include-wip` can't
    /// add.
    UntrackedFiles(Vec<String>),
    Submodules,
    Lfs,
    TooLarge {
        size: u64,
        limit_mb: NonZeroU64,
    },
    /// Holds the output's path relative to the repo root.
    UnignoredOutput(PathBuf),
    /// Holds the previous bundle, whose refs couldn't be read for the up-to-date check.
    UnreadableBundle(PathBuf),
    /// Holds how many bundles a config-only prune would have deleted, had it been able to ask.
    PruneSkipped {
        count: usize,
        reason: PromptUnavailable,
    },
}

/// Why a run can't ask before deleting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptUnavailable {
    NoTerminal,
    /// `--json` output is for scripts, so it never stops to ask.
    Json,
}

impl fmt::Display for PromptUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTerminal => formatter.write_str("there is no terminal to confirm"),
            Self::Json => formatter.write_str("`--json` never asks to confirm"),
        }
    }
}

impl Warning {
    /// Name the warning in a form scripts can match on, which stays fixed when the message changes.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UncommittedChanges(_) => "uncommitted_changes",
            Self::UntrackedFiles(_) => "untracked_files",
            Self::Submodules => "submodules",
            Self::Lfs => "lfs",
            Self::TooLarge { .. } => "too_large",
            Self::UnignoredOutput(_) => "unignored_output",
            Self::UnreadableBundle(_) => "unreadable_bundle",
            Self::PruneSkipped { .. } => "prune_skipped",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UncommittedChanges(changes) => {
                formatter.write_str("uncommitted changes are not in the bundle")?;
                if changes.iter().any(|change| !is_untracked(change)) {
                    formatter.write_str("; pass --include-wip to add changes to tracked files")?;
                }
                write_changes(formatter, changes)
            }
            Self::UntrackedFiles(files) => {
                formatter.write_str("untracked files are not in the bundle, even with --include-wip")?;
                write_changes(formatter, files)
            }
            Self::Submodules => formatter.write_str(
                "submodule contents are not in the bundle, only the commits each submodule points to",
            ),
            Self::Lfs => formatter
                .write_str("Git LFS file contents are not in the bundle, only their pointer files"),
            Self::TooLarge { size, limit_mb } => write!(
                formatter,
                "the bundle is {}, over the `max_size_mb` limit of {limit_mb} MB",
                format_size(*size)
            ),
            Self::UnignoredOutput(path) => write!(
                formatter,
                "the output {} is inside the repo but not ignored, so git lists it as untracked; add it to .gitignore or .git/info/exclude",
                path.display()
            ),
            Self::UnreadableBundle(path) => write!(
                formatter,
                "the bundle {} can't be read, so it wasn't reused; run `git bundle verify` on it to see why",
                display_path(path)
            ),
            Self::PruneSkipped { count, reason } => write!(
                formatter,
                "pruning would delete {}, but {reason}, so nothing was deleted; pass --yes to delete without asking",
                count_bundles(*count)
            ),
        }
    }
}

/// Return warnings about repo state that a bundle can't capture. Leave the output out of the
/// uncommitted changes, since the unignored-output warning already covers it.
pub fn repo_warnings(git: &Git, output: &Output, include_wip: bool) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();
    let output_path = output_in_repo(output, git)?;
    let changes = git.status_lines(output_path.as_deref())?;
    warnings.extend(changes_warning(changes, include_wip));
    if git.repo_dir().join(SUBMODULES_FILE).is_file() {
        warnings.push(Warning::Submodules);
    }
    if uses_lfs(git.repo_dir())? {
        warnings.push(Warning::Lfs);
    }
    Ok(warnings)
}

/// Warn when `bundle` exists and is larger than `limit_mb`.
pub fn size_warning(bundle: &Path, limit_mb: NonZeroU64) -> Result<Option<Warning>> {
    if !bundle.is_file() {
        return Ok(None);
    }
    let size = fs::metadata(bundle)
        .with_context(|| format!("failed to read the size of {}", bundle.display()))?
        .len();
    if size <= limit_mb.get().saturating_mul(BYTES_PER_MB) {
        return Ok(None);
    }
    Ok(Some(Warning::TooLarge { size, limit_mb }))
}

/// Warn when the output sits inside the repo where git doesn't ignore it.
pub fn unignored_output(git: &Git, output: &Output, bundle: &Path) -> Result<Option<Warning>> {
    let Some(path) = output_in_repo(output, git)? else {
        return Ok(None);
    };
    if git.is_ignored(&ignore_probe(output, &path, bundle))? {
        return Ok(None);
    }
    Ok(Some(Warning::UnignoredOutput(path)))
}

/// Return the repo-relative path whose ignore status decides the warning. For a directory that's
/// the bundle inside it, since a `.gitignore` in the directory ignores its files but not the
/// directory itself.
fn ignore_probe(output: &Output, output_path: &Path, bundle: &Path) -> PathBuf {
    match (output, bundle.file_name()) {
        (Output::Directory(_), Some(name)) => output_path.join(name),
        _ => output_path.to_path_buf(),
    }
}

/// Return the output's path relative to the repo root when it exists strictly inside the repo.
fn output_in_repo(output: &Output, git: &Git) -> Result<Option<PathBuf>> {
    let target = match output {
        Output::Directory(dir) => &dir.path,
        Output::File(path) => path,
    };
    git.path_in_repo(target)
}

/// Warn about every change the bundle leaves out. `--include-wip` bundles the changes to tracked
/// files, so with it only untracked files are left out.
fn changes_warning(changes: Vec<String>, include_wip: bool) -> Option<Warning> {
    if !include_wip {
        if changes.is_empty() {
            return None;
        }
        return Some(Warning::UncommittedChanges(changes));
    }
    let untracked: Vec<String> = changes
        .into_iter()
        .filter(|change| is_untracked(change))
        .collect();
    if untracked.is_empty() {
        return None;
    }
    Some(Warning::UntrackedFiles(untracked))
}

fn is_untracked(change: &str) -> bool {
    change.starts_with(UNTRACKED_PREFIX)
}

/// List each change on its own line under the message, up to `MAX_LISTED_CHANGES`.
fn write_changes(formatter: &mut fmt::Formatter<'_>, changes: &[String]) -> fmt::Result {
    for change in changes.iter().take(MAX_LISTED_CHANGES) {
        write!(formatter, "\n  {change}")?;
    }
    let hidden = changes.len().saturating_sub(MAX_LISTED_CHANGES);
    if hidden > 0 {
        write!(formatter, "\n  and {hidden} more")?;
    }
    Ok(())
}

fn uses_lfs(repo_dir: &Path) -> Result<bool> {
    let path = repo_dir.join(ATTRIBUTES_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    Ok(text.lines().any(declares_lfs))
}

/// Skip comments, so a commented-out LFS rule doesn't count.
fn declares_lfs(line: &str) -> bool {
    let line = line.trim_start();
    !line.starts_with('#') && line.split_whitespace().any(|token| token == LFS_ATTRIBUTE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundles::BundleDir;
    use crate::naming::DEFAULT_TEMPLATE;
    use crate::test_support::test_repo;
    use tempfile::TempDir;

    fn directory_output(path: PathBuf) -> Output {
        Output::Directory(BundleDir {
            path,
            name: DEFAULT_TEMPLATE.parse().unwrap(),
        })
    }

    fn default_output(repo: &Path) -> Output {
        directory_output(repo.join("bundles"))
    }

    fn limit(megabytes: u64) -> NonZeroU64 {
        NonZeroU64::new(megabytes).unwrap()
    }

    fn changes(count: usize) -> Warning {
        let lines = (0..count).map(|index| format!("?? file{index}")).collect();
        Warning::UncommittedChanges(lines)
    }

    fn warnings_with_file(name: &str, contents: &str) -> Vec<Warning> {
        let (dir, git) = test_repo();
        fs::write(dir.path().join(name), contents).unwrap();
        repo_warnings(&git, &default_output(dir.path()), false).unwrap()
    }

    // Repo state

    #[test]
    fn clean_repo_has_no_warnings() {
        let (dir, git) = test_repo();
        let warnings = repo_warnings(&git, &default_output(dir.path()), false).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn untracked_file_is_an_uncommitted_change() {
        let expected = Warning::UncommittedChanges(vec!["?? notes.txt".to_string()]);
        assert_eq!(warnings_with_file("notes.txt", "draft"), [expected]);
    }

    #[test]
    fn output_is_left_out_of_uncommitted_changes() {
        let (dir, git) = test_repo();
        fs::create_dir(dir.path().join("bundles")).unwrap();
        fs::write(dir.path().join("bundles/old.bundle"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        let expected = Warning::UncommittedChanges(vec!["?? notes.txt".to_string()]);
        let warnings = repo_warnings(&git, &default_output(dir.path()), false).unwrap();
        assert_eq!(warnings, [expected]);
    }

    #[test]
    fn gitmodules_file_means_submodules() {
        let warnings = warnings_with_file(SUBMODULES_FILE, "");
        assert!(warnings.contains(&Warning::Submodules), "{warnings:?}");
    }

    #[test]
    fn lfs_filter_in_gitattributes_means_lfs() {
        let warnings = warnings_with_file(ATTRIBUTES_FILE, "*.psd filter=lfs diff=lfs -text\n");
        assert!(warnings.contains(&Warning::Lfs), "{warnings:?}");
    }

    #[test]
    fn commented_out_lfs_rule_is_ignored() {
        let warnings = warnings_with_file(ATTRIBUTES_FILE, "# *.psd filter=lfs\n");
        assert!(!warnings.contains(&Warning::Lfs), "{warnings:?}");
    }

    // Size

    #[test]
    fn bundle_over_the_limit_is_too_large() {
        let dir = TempDir::new().unwrap();
        let bundle = dir.path().join("x.bundle");
        fs::write(&bundle, vec![0u8; 1_500_000]).unwrap();

        let expected = Warning::TooLarge {
            size: 1_500_000,
            limit_mb: limit(1),
        };
        assert_eq!(size_warning(&bundle, limit(1)).unwrap(), Some(expected));
        assert_eq!(size_warning(&bundle, limit(2)).unwrap(), None);
    }

    #[test]
    fn missing_bundle_has_no_size_warning() {
        let dir = TempDir::new().unwrap();
        let bundle = dir.path().join("x.bundle");
        assert_eq!(size_warning(&bundle, limit(1)).unwrap(), None);
    }

    // Unignored output

    fn unignored_in_default_dir(repo: &Path) -> Option<Warning> {
        let (git, output) = (Git::new(repo), default_output(repo));
        let bundle = repo.join("bundles/a.bundle");
        unignored_output(&git, &output, &bundle).unwrap()
    }

    #[test]
    fn unignored_output_directory_is_reported() {
        let (dir, _git) = test_repo();
        fs::create_dir(dir.path().join("bundles")).unwrap();

        let expected = Warning::UnignoredOutput(PathBuf::from("bundles"));
        assert_eq!(unignored_in_default_dir(dir.path()), Some(expected));
    }

    #[test]
    fn ignored_output_directory_is_not_reported() {
        let (dir, _git) = test_repo();
        fs::write(dir.path().join(".gitignore"), "/bundles/\n").unwrap();
        fs::create_dir(dir.path().join("bundles")).unwrap();

        assert_eq!(unignored_in_default_dir(dir.path()), None);
    }

    #[test]
    fn directory_that_ignores_its_own_files_is_not_reported() {
        let (dir, _git) = test_repo();
        fs::create_dir(dir.path().join("bundles")).unwrap();
        fs::write(dir.path().join("bundles/.gitignore"), "*\n").unwrap();

        assert_eq!(unignored_in_default_dir(dir.path()), None);
    }

    #[test]
    fn unignored_output_file_is_reported() {
        let (dir, git) = test_repo();
        let file = dir.path().join("snapshot.bundle");
        fs::write(&file, "").unwrap();

        let expected = Warning::UnignoredOutput(PathBuf::from("snapshot.bundle"));
        assert_eq!(
            unignored_output(&git, &Output::File(file.clone()), &file).unwrap(),
            Some(expected)
        );
    }

    #[test]
    fn missing_output_is_not_reported() {
        let (dir, _git) = test_repo();
        assert_eq!(unignored_in_default_dir(dir.path()), None);
    }

    #[test]
    fn output_outside_the_repo_is_not_reported() {
        let (dir, git) = test_repo();
        let sibling = TempDir::new_in(dir.path().parent().unwrap()).unwrap();
        let through_parent = dir
            .path()
            .join("..")
            .join(sibling.path().file_name().unwrap());

        let bundle = through_parent.join("a.bundle");
        let output = directory_output(through_parent);
        assert_eq!(unignored_output(&git, &output, &bundle).unwrap(), None);
    }

    #[test]
    fn repo_root_as_output_is_not_reported() {
        let (dir, git) = test_repo();
        let output = directory_output(dir.path().to_path_buf());
        let bundle = dir.path().join("a.bundle");
        assert_eq!(unignored_output(&git, &output, &bundle).unwrap(), None);
    }

    // Messages

    #[test]
    fn lists_at_most_ten_changes() {
        let message = changes(12).to_string();
        assert!(message.contains("?? file9"), "{message}");
        assert!(!message.contains("?? file10"), "{message}");
        assert!(message.ends_with("and 2 more"), "{message}");
    }

    #[test]
    fn lists_every_change_when_there_are_few() {
        let message = changes(2).to_string();
        assert!(message.contains("?? file1"), "{message}");
        assert!(!message.contains("more"), "{message}");
    }

    #[test]
    fn tracked_changes_suggest_include_wip() {
        let warning = Warning::UncommittedChanges(vec![" M src/main.rs".to_string()]);
        let message = warning.to_string();
        assert!(message.contains("--include-wip"), "{message}");
    }

    #[test]
    fn untracked_files_alone_do_not_suggest_include_wip() {
        let message = changes(2).to_string();
        assert!(!message.contains("--include-wip"), "{message}");
    }

    #[test]
    fn include_wip_leaves_only_untracked_files_to_warn_about() {
        let lines = vec![" M tracked.txt".to_string(), "?? new.txt".to_string()];
        let expected = Warning::UntrackedFiles(vec!["?? new.txt".to_string()]);
        assert_eq!(changes_warning(lines, true), Some(expected));
    }

    #[test]
    fn include_wip_with_only_tracked_changes_has_no_warning() {
        let lines = vec![" M tracked.txt".to_string(), "A  staged.txt".to_string()];
        assert_eq!(changes_warning(lines, true), None);
    }

    #[test]
    fn size_message_shows_both_sizes_in_megabytes() {
        let warning = Warning::TooLarge {
            size: 42_300_000,
            limit_mb: limit(30),
        };
        let message = warning.to_string();
        assert!(message.contains("42.3 MB"), "{message}");
        assert!(message.contains("30 MB"), "{message}");
    }

    #[test]
    fn skipped_prune_message_counts_bundles_and_names_yes() {
        let warning = Warning::PruneSkipped {
            count: 2,
            reason: PromptUnavailable::NoTerminal,
        };
        let message = warning.to_string();
        assert!(message.contains("2 bundles"), "{message}");
        assert!(message.contains("no terminal"), "{message}");
        assert!(message.contains("--yes"), "{message}");
    }

    #[test]
    fn skipped_prune_message_blames_json_when_json_is_why() {
        let warning = Warning::PruneSkipped {
            count: 1,
            reason: PromptUnavailable::Json,
        };
        let message = warning.to_string();
        assert!(message.contains("`--json` never asks"), "{message}");
    }

    #[test]
    fn size_message_uses_larger_units_for_large_bundles() {
        let warning = Warning::TooLarge {
            size: 1_500_000_000,
            limit_mb: limit(30),
        };
        let message = warning.to_string();
        assert!(message.contains("1.5 GB"), "{message}");
    }
}
