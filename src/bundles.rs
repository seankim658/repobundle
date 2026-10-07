use std::cmp::Ordering;
use std::fs::{self, DirEntry, ReadDir};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::config::RepoName;
use crate::naming::{BundleNameMatcher, NameTemplate};

/// A bundle of this repo found in an output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub path: PathBuf,
    /// Present only when the template contains `{timestamp}`.
    pub created_at: Option<DateTime<Utc>>,
    pub modified: SystemTime,
}

/// Return the files in `dir` that `matcher` accepts, newest first. A missing directory has no
/// bundles. Order by the timestamp in the name, then by mtime, then by path, so the order never
/// depends on how the directory happens to list.
pub fn find(dir: &Path, matcher: &BundleNameMatcher) -> Result<Vec<BundleFile>> {
    let Some(entries) = read_dir_if_exists(dir)? else {
        return Ok(Vec::new());
    };
    let mut bundles = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("failed to read bundle directory {}", dir.display()))?;
        if let Some(bundle) = read_bundle(&entry, matcher)? {
            bundles.push(bundle);
        }
    }
    bundles.sort_by(newest_first);
    Ok(bundles)
}

/// A directory of bundles whose names come from `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleDir {
    pub path: PathBuf,
    pub name: NameTemplate,
}

impl BundleDir {
    /// Return this repo's bundles in the directory, newest first.
    pub fn find(&self, repo: &RepoName) -> Result<Vec<BundleFile>> {
        find(&self.path, &matcher(&self.name, repo.as_str())?)
    }
}

pub fn matcher(template: &NameTemplate, repo: &str) -> Result<BundleNameMatcher> {
    template
        .matcher(repo)
        .context("failed to build the bundle name pattern")
}

fn read_dir_if_exists(dir: &Path) -> Result<Option<ReadDir>> {
    match fs::read_dir(dir) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read bundle directory {}", dir.display()))
        }
    }
}

/// Skip anything the matcher rejects or that isn't a regular file, so no caller ever reads,
/// compares, or deletes it. Symlinks are skipped too.
fn read_bundle(entry: &DirEntry, matcher: &BundleNameMatcher) -> Result<Option<BundleFile>> {
    let file_name = entry.file_name();
    let Some(parsed) = file_name.to_str().and_then(|name| matcher.parse(name)) else {
        return Ok(None);
    };
    let path = entry.path();
    let metadata = entry
        .metadata()
        .with_context(|| format!("failed to read {}", path.display()))?;
    if !metadata.is_file() {
        return Ok(None);
    }
    let modified = metadata
        .modified()
        .with_context(|| format!("failed to read the modification time of {}", path.display()))?;
    Ok(Some(BundleFile {
        path,
        created_at: parsed.created_at,
        modified,
    }))
}

fn newest_first(left: &BundleFile, right: &BundleFile) -> Ordering {
    right
        .created_at
        .cmp(&left.created_at)
        .then_with(|| right.modified.cmp(&left.modified))
        .then_with(|| right.path.cmp(&left.path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{DEFAULT_TEMPLATE, NameTemplate};
    use std::fs::File;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::TempDir;

    const REPO: &str = "myrepo";
    const NO_TIMESTAMP_TEMPLATE: &str = "{date}-{branch}@{repo}.bundle";

    fn matcher(template: &str) -> BundleNameMatcher {
        let template: NameTemplate = template.parse().unwrap();
        template.matcher(REPO).unwrap()
    }

    /// Create an empty file whose mtime is `seconds` after the Unix epoch.
    fn touch(dir: &Path, name: &str, seconds: u64) {
        let file = File::create(dir.join(name)).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    fn found(dir: &Path, template: &str) -> Vec<String> {
        let bundles = find(dir, &matcher(template)).unwrap();
        bundles
            .iter()
            .map(|bundle| {
                bundle
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn missing_directory_has_no_bundles() {
        let dir = TempDir::new().unwrap();
        assert!(found(&dir.path().join("absent"), DEFAULT_TEMPLATE).is_empty());
    }

    #[test]
    fn timestamp_in_name_outranks_mtime() {
        let dir = TempDir::new().unwrap();
        touch(dir.path(), "20261004T153012Z-a1b2c3d-myrepo.bundle", 200);
        touch(dir.path(), "20261005T090000Z-b2c3d4e-myrepo.bundle", 100);

        assert_eq!(
            found(dir.path(), DEFAULT_TEMPLATE),
            [
                "20261005T090000Z-b2c3d4e-myrepo.bundle",
                "20261004T153012Z-a1b2c3d-myrepo.bundle",
            ]
        );
    }

    #[test]
    fn same_second_ties_go_to_the_newer_mtime() {
        let dir = TempDir::new().unwrap();
        touch(dir.path(), "20261004T153012Z-a1b2c3d-myrepo.bundle", 200);
        touch(dir.path(), "20261004T153012Z-b2c3d4e-myrepo.bundle", 100);

        assert_eq!(
            found(dir.path(), DEFAULT_TEMPLATE),
            [
                "20261004T153012Z-a1b2c3d-myrepo.bundle",
                "20261004T153012Z-b2c3d4e-myrepo.bundle",
            ]
        );
    }

    #[test]
    fn template_without_timestamp_orders_by_mtime() {
        let dir = TempDir::new().unwrap();
        touch(dir.path(), "20261004-main@myrepo.bundle", 100);
        touch(dir.path(), "20261004-feature@myrepo.bundle", 200);

        assert_eq!(
            found(dir.path(), NO_TIMESTAMP_TEMPLATE),
            [
                "20261004-feature@myrepo.bundle",
                "20261004-main@myrepo.bundle"
            ]
        );
    }

    #[test]
    fn full_ties_order_the_same_every_time() {
        let dir = TempDir::new().unwrap();
        touch(dir.path(), "20261004T153012Z-a1b2c3d-myrepo.bundle", 100);
        touch(dir.path(), "20261004T153012Z-b2c3d4e-myrepo.bundle", 100);

        assert_eq!(
            found(dir.path(), DEFAULT_TEMPLATE),
            [
                "20261004T153012Z-b2c3d4e-myrepo.bundle",
                "20261004T153012Z-a1b2c3d-myrepo.bundle",
            ]
        );
    }

    #[test]
    fn skips_everything_but_this_repos_bundle_files() {
        let dir = TempDir::new().unwrap();
        let bundle = "20261004T153012Z-a1b2c3d-myrepo.bundle";
        touch(dir.path(), bundle, 100);
        touch(dir.path(), "20261004T153012Z-a1b2c3d-other.bundle", 100);
        touch(
            dir.path(),
            "20261004T153012Z-b2c3d4e-myrepo.bundle.tmp",
            100,
        );
        touch(dir.path(), "notes.txt", 100);
        fs::create_dir(dir.path().join("20261004T153012Z-c3d4e5f-myrepo.bundle")).unwrap();

        assert_eq!(found(dir.path(), DEFAULT_TEMPLATE), [bundle]);
    }
}
