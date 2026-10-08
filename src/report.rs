use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use chrono::SecondsFormat;

use crate::bundles::BundleFile;
use crate::create::RefMatch;
use crate::output;
use crate::settings::Output;
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

/// One bundle in a listing, with how its refs compare to the repo's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedBundle {
    pub bundle: BundleFile,
    pub refs: RefMatch,
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
    /// Show the bundles in `output`, newest first. A listing is the whole result of its run.
    fn list(&mut self, output: &Output, bundles: &[ListedBundle]);

    /// Note that a create left nothing over the prune limit. Text output says nothing here, so a
    /// routine create prints only its result.
    fn nothing_over_limit(&mut self) {}

    /// Note that a prune from config was skipped for want of a prompt. Text output relies on the
    /// warning that comes with it.
    fn prune_skipped(&mut self) {}
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

    fn list(&mut self, output: &Output, bundles: &[ListedBundle]) {
        output::listing(output, bundles);
    }
}

/// Collect every result into one object, to print once the run succeeds. Paths are absolute, so
/// a script doesn't need to know where the run started.
#[derive(Debug, Default)]
pub struct JsonReporter {
    run: JsonRun,
    /// Present only for `--list`, which prints this instead of `run`.
    listing: Option<JsonListing>,
}

#[derive(Debug, Default, Serialize)]
struct JsonRun {
    bundle: Option<JsonBundle>,
    warnings: Vec<JsonWarning>,
    prune: JsonPrune,
}

#[derive(Debug, Serialize)]
struct JsonListing {
    bundles: Vec<JsonListedBundle>,
}

#[derive(Debug, Serialize)]
struct JsonListedBundle {
    path: PathBuf,
    size: u64,
    /// UTC, in RFC 3339 form such as `2026-10-07T15:02:11Z`.
    created_at: String,
    status: RefMatch,
}

impl From<&ListedBundle> for JsonListedBundle {
    fn from(listed: &ListedBundle) -> Self {
        Self {
            path: listed.bundle.path.clone(),
            size: listed.bundle.size,
            created_at: listed
                .bundle
                .created()
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            status: listed.refs,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonBundle {
    status: BundleStatus,
    path: PathBuf,
    /// Present only for a bundle this run wrote.
    size: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum BundleStatus {
    Created,
    UpToDate,
    WouldCreate,
}

#[derive(Debug, Serialize)]
struct JsonWarning {
    kind: &'static str,
    message: String,
}

#[derive(Debug, Default, Serialize)]
struct JsonPrune {
    status: PruneStatus,
    /// The bundles deleted, or the ones a dry run would delete.
    paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PruneStatus {
    /// No prune was asked for.
    #[default]
    None,
    NothingToPrune,
    WouldDelete,
    Deleted,
    Skipped,
    Declined,
}

impl JsonReporter {
    pub fn to_json(&self) -> Result<String> {
        let json = match &self.listing {
            Some(listing) => serde_json::to_string_pretty(listing),
            None => serde_json::to_string_pretty(&self.run),
        };
        json.context("failed to write the result as JSON")
    }

    pub fn print(&self) -> Result<()> {
        println!("{}", self.to_json()?);
        Ok(())
    }
}

impl Reporter for JsonReporter {
    fn bundle(&mut self, outcome: &BundleOutcome) {
        let (status, size) = match outcome {
            BundleOutcome::Created { size, .. } => (BundleStatus::Created, Some(*size)),
            BundleOutcome::UpToDate(_) => (BundleStatus::UpToDate, None),
            BundleOutcome::WouldCreate(_) => (BundleStatus::WouldCreate, None),
        };
        self.run.bundle = Some(JsonBundle {
            status,
            path: outcome.path().to_path_buf(),
            size,
        });
    }

    fn warnings(&mut self, warnings: &[Warning]) {
        self.run
            .warnings
            .extend(warnings.iter().map(|warning| JsonWarning {
                kind: warning.kind(),
                message: output::capitalize(&warning.to_string()),
            }));
    }

    fn nothing_to_prune(&mut self) {
        self.run.prune.status = PruneStatus::NothingToPrune;
    }

    fn would_delete(&mut self, bundles: &[BundleFile]) {
        self.run.prune.status = PruneStatus::WouldDelete;
        self.run.prune.paths = bundles.iter().map(|bundle| bundle.path.clone()).collect();
    }

    fn deleted(&mut self, path: &Path) {
        self.run.prune.status = PruneStatus::Deleted;
        self.run.prune.paths.push(path.to_path_buf());
    }

    fn nothing_deleted(&mut self) {
        self.run.prune.status = PruneStatus::Declined;
    }

    fn nothing_over_limit(&mut self) {
        self.run.prune.status = PruneStatus::NothingToPrune;
    }

    fn prune_skipped(&mut self) {
        self.run.prune.status = PruneStatus::Skipped;
    }

    fn list(&mut self, _output: &Output, bundles: &[ListedBundle]) {
        let bundles = bundles.iter().map(JsonListedBundle::from).collect();
        self.listing = Some(JsonListing { bundles });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::time::UNIX_EPOCH;

    fn parsed(reporter: &JsonReporter) -> Value {
        serde_json::from_str(&reporter.to_json().unwrap()).unwrap()
    }

    fn bundle_file(path: &str) -> BundleFile {
        BundleFile {
            path: PathBuf::from(path),
            created_at: None,
            modified: UNIX_EPOCH,
            size: 0,
        }
    }

    #[test]
    fn empty_report_has_no_bundle_and_no_prune() {
        let expected = json!({
            "bundle": null,
            "warnings": [],
            "prune": { "status": "none", "paths": [] }
        });
        assert_eq!(parsed(&JsonReporter::default()), expected);
    }

    #[test]
    fn created_bundle_has_its_size() {
        let mut reporter = JsonReporter::default();
        reporter.bundle(&BundleOutcome::Created {
            path: PathBuf::from("/out/a.bundle"),
            size: 149_000,
        });

        let expected = json!({ "status": "created", "path": "/out/a.bundle", "size": 149_000 });
        assert_eq!(parsed(&reporter)["bundle"], expected);
    }

    #[test]
    fn reused_bundle_has_no_size() {
        let mut reporter = JsonReporter::default();
        reporter.bundle(&BundleOutcome::UpToDate(PathBuf::from("/out/a.bundle")));

        let expected = json!({ "status": "up_to_date", "path": "/out/a.bundle", "size": null });
        assert_eq!(parsed(&reporter)["bundle"], expected);
    }

    #[test]
    fn warnings_keep_their_kind_and_read_as_sentences() {
        let mut reporter = JsonReporter::default();
        reporter.warnings(&[Warning::Lfs]);

        let warning = &parsed(&reporter)["warnings"][0];
        assert_eq!(warning["kind"], "lfs");
        assert!(warning["message"].as_str().unwrap().starts_with("Git LFS"));
    }

    #[test]
    fn deletions_collect_into_one_list() {
        let mut reporter = JsonReporter::default();
        reporter.deleted(Path::new("/out/b.bundle"));
        reporter.deleted(Path::new("/out/a.bundle"));

        let expected = json!({ "status": "deleted", "paths": ["/out/b.bundle", "/out/a.bundle"] });
        assert_eq!(parsed(&reporter)["prune"], expected);
    }

    #[test]
    fn listing_replaces_the_run_object() {
        let mut reporter = JsonReporter::default();
        let listed = ListedBundle {
            bundle: bundle_file("/out/a.bundle"),
            refs: RefMatch::OutOfDate,
        };
        reporter.list(&Output::File(PathBuf::from("/out/a.bundle")), &[listed]);

        let expected = json!({
            "bundles": [{
                "path": "/out/a.bundle",
                "size": 0,
                "created_at": "1970-01-01T00:00:00Z",
                "status": "out_of_date"
            }]
        });
        assert_eq!(parsed(&reporter), expected);
    }

    #[test]
    fn dry_run_lists_what_would_be_deleted() {
        let mut reporter = JsonReporter::default();
        reporter.would_delete(&[bundle_file("/out/a.bundle")]);

        let expected = json!({ "status": "would_delete", "paths": ["/out/a.bundle"] });
        assert_eq!(parsed(&reporter)["prune"], expected);
    }
}
