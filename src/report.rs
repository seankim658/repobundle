use std::path::{Path, PathBuf};

use crate::bundles::BundleFile;
use crate::output;
use crate::warnings::Warning;

/// What happened to the bundle a create run points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleOutcome {
    Created {
        path: PathBuf,
        size: u64,
    },
    /// The previous bundle already held the current refs, so it was reused.
    UpToDate(PathBuf),
    /// A dry run planned this path without writing it.
    WouldCreate(PathBuf),
}

impl BundleOutcome {
    pub fn path(&self) -> &Path {
        match self {
            Self::Created { path, .. } | Self::UpToDate(path) | Self::WouldCreate(path) => path,
        }
    }
}

/// Receive each result of a run as it happens, so text output can show the result line before a
/// delete prompt, and a failed delete still reports the bundles deleted before it.
pub trait Reporter {
    fn bundle(&mut self, outcome: &BundleOutcome);
    fn warnings(&mut self, warnings: &[Warning]);
    fn nothing_to_prune(&mut self);
    fn would_delete(&mut self, bundles: &[BundleFile]);
    fn deleted(&mut self, path: &Path);
    fn nothing_deleted(&mut self);
}

/// Print each result right away, in the badge format.
pub struct TextReporter;

impl Reporter for TextReporter {
    fn bundle(&mut self, outcome: &BundleOutcome) {
        match outcome {
            BundleOutcome::Created { path, size } => output::created(path, *size),
            BundleOutcome::UpToDate(path) => output::up_to_date(path),
            BundleOutcome::WouldCreate(path) => output::would_create(path),
        }
    }

    fn warnings(&mut self, warnings: &[Warning]) {
        output::warnings(warnings);
    }

    fn nothing_to_prune(&mut self) {
        output::nothing_to_prune();
    }

    fn would_delete(&mut self, bundles: &[BundleFile]) {
        output::would_delete(bundles);
    }

    fn deleted(&mut self, path: &Path) {
        output::deleted(path);
    }

    fn nothing_deleted(&mut self) {
        output::nothing_deleted();
    }
}
