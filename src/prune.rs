use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use anyhow::{Context, Result};

use crate::bundles::BundleFile;

/// Return the bundles to delete so only the newest `keep` remain, given `bundles` newest first.
/// The `protected` bundle always counts as one of those kept, whether or not it exists yet and
/// wherever it sorts.
pub fn select(
    bundles: Vec<BundleFile>,
    keep: NonZeroUsize,
    protected: Option<&Path>,
) -> Vec<BundleFile> {
    let reserved = usize::from(protected.is_some());
    bundles
        .into_iter()
        .filter(|bundle| Some(bundle.path.as_path()) != protected)
        .skip(keep.get() - reserved)
        .collect()
}

pub fn delete(bundle: &BundleFile) -> Result<()> {
    fs::remove_file(&bundle.path)
        .with_context(|| format!("failed to delete {}", bundle.path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    const DIR: &str = "/out";

    fn bundle(name: &str) -> BundleFile {
        BundleFile {
            path: Path::new(DIR).join(name),
            created_at: None,
            modified: UNIX_EPOCH,
            size: 0,
        }
    }

    fn bundles(names: &[&str]) -> Vec<BundleFile> {
        names.iter().map(|name| bundle(name)).collect()
    }

    fn keep(count: usize) -> NonZeroUsize {
        NonZeroUsize::new(count).unwrap()
    }

    fn selected(names: &[&str], count: usize, protected: Option<&str>) -> Vec<String> {
        let protected: Option<PathBuf> = protected.map(|name| Path::new(DIR).join(name));
        select(bundles(names), keep(count), protected.as_deref())
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
    fn deletes_everything_after_the_newest_few() {
        assert_eq!(selected(&["d", "c", "b", "a"], 2, None), ["b", "a"]);
    }

    #[test]
    fn deletes_nothing_when_keeping_more_than_exist() {
        assert!(selected(&["b", "a"], 5, None).is_empty());
    }

    #[test]
    fn unwritten_protected_bundle_takes_a_kept_slot() {
        assert_eq!(selected(&["c", "b", "a"], 2, Some("new")), ["b", "a"]);
    }

    #[test]
    fn protected_bundle_survives_even_when_it_sorts_last() {
        assert_eq!(selected(&["c", "b", "a"], 1, Some("a")), ["c", "b"]);
    }

    #[test]
    fn protected_newest_bundle_is_counted_once() {
        assert_eq!(selected(&["new", "b", "a"], 2, Some("new")), ["a"]);
    }
}
