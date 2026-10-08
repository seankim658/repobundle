use std::io::{self, IsTerminal};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::bundles::{self, BundleDir, BundleFile};
use crate::create::{self, RefMatch};
use crate::git::{Git, RefSet};
use crate::output;
use crate::prompt;
use crate::prune;
use crate::report::{BundleOutcome, ListedBundle, Reporter};
use crate::settings::{Action, Output, PruneLimit, PruneOrigin, Settings};
use crate::warnings::{self, PromptUnavailable, Warning};

/// Carries out one run against a repo, sending each result to a reporter.
pub struct Runner<'a> {
    git: &'a Git,
    settings: &'a Settings,
    reporter: &'a mut dyn Reporter,
}

impl<'a> Runner<'a> {
    pub fn new(git: &'a Git, settings: &'a Settings, reporter: &'a mut dyn Reporter) -> Self {
        Self {
            git,
            settings,
            reporter,
        }
    }

    /// Do what the settings' action says.
    pub fn run(&mut self) -> Result<()> {
        let settings = self.settings;
        match &settings.action {
            Action::Create(output) => self.create(output).map(drop),
            Action::CreateAndPrune { dir, limit } => self.create_and_prune(dir, *limit),
            Action::PruneOnly { dir, keep } => self.prune_only(dir, *keep),
            Action::List(output) => self.list(output),
        }
    }

    /// Compare every bundle with the repo's refs, read once for the whole listing.
    fn list(&mut self, output: &Output) -> Result<()> {
        let bundles = self.find_bundles(output)?;
        if bundles.is_empty() {
            self.reporter.list(output, &[]);
            return Ok(());
        }
        self.git.ensure_has_commits()?;
        let current = self
            .git
            .current_refs(&self.git.ref_set(self.settings.refs)?)?;
        let listed: Vec<ListedBundle> = bundles
            .into_iter()
            .map(|bundle| ListedBundle {
                refs: create::compare_refs(self.git, &bundle.path, &current),
                bundle,
            })
            .collect();
        self.reporter.list(output, &listed);
        Ok(())
    }

    fn find_bundles(&self, output: &Output) -> Result<Vec<BundleFile>> {
        match output {
            Output::Directory(dir) => dir.find(&self.settings.repo_name),
            Output::File(path) => Ok(bundles::read_file(path)?.into_iter().collect()),
        }
    }

    /// Check the repo before creating, so the warnings describe the state that was bundled.
    /// Return the bundle this run points to.
    fn create(&mut self, output: &Output) -> Result<PathBuf> {
        let mut warnings = self.repo_warnings(output)?;
        let outcome = self.create_bundle(output, &mut warnings)?;
        self.reporter.bundle(&outcome);
        warnings.extend(self.bundle_warnings(output, outcome.path())?);
        self.reporter.warnings(&warnings);
        Ok(outcome.path().to_path_buf())
    }

    /// Prune with this run's bundle protected, so it always counts as one of those kept.
    fn create_and_prune(&mut self, dir: &BundleDir, limit: PruneLimit) -> Result<()> {
        let bundle = self.create(&Output::Directory(dir.clone()))?;
        let found = dir.find(&self.settings.repo_name)?;
        let excess = prune::select(found, limit.keep, Some(&bundle));
        if excess.is_empty() {
            self.reporter.nothing_over_limit();
            return Ok(());
        }
        self.remove_excess(&excess, limit.origin)
    }

    fn repo_warnings(&self, output: &Output) -> Result<Vec<Warning>> {
        if !self.settings.warnings {
            return Ok(Vec::new());
        }
        warnings::repo_warnings(self.git, output)
    }

    fn bundle_warnings(&self, output: &Output, bundle: &Path) -> Result<Vec<Warning>> {
        if !self.settings.warnings {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        if let Some(limit_mb) = self.settings.max_size_mb {
            found.extend(warnings::size_warning(bundle, limit_mb)?);
        }
        found.extend(warnings::unignored_output(self.git, output, bundle)?);
        Ok(found)
    }

    /// Return what happened to the bundle this run points to. Add a warning when the previous
    /// bundle couldn't be read.
    fn create_bundle(&self, output: &Output, warnings: &mut Vec<Warning>) -> Result<BundleOutcome> {
        let settings = self.settings;
        let refs = self.git.ref_set(settings.refs)?;
        let path = create::bundle_path(self.git, output, &settings.repo_name, &refs)?;
        match self.compare_with_previous(output, &refs)? {
            Some((existing, RefMatch::Current)) => return Ok(BundleOutcome::UpToDate(existing)),
            Some((previous, RefMatch::Unreadable)) if settings.warnings => {
                warnings.push(Warning::UnreadableBundle(previous));
            }
            _ => {}
        }
        if settings.dry_run {
            return Ok(BundleOutcome::WouldCreate(path));
        }
        let spinner = self.reporter.writing(&path);
        let size = create::write_bundle(self.git, &path, &refs)?;
        drop(spinner);
        Ok(BundleOutcome::Created { path, size })
    }

    /// Compare the previous bundle with the current refs, unless `--force` skips the check.
    fn compare_with_previous(
        &self,
        output: &Output,
        refs: &RefSet,
    ) -> Result<Option<(PathBuf, RefMatch)>> {
        if self.settings.force {
            return Ok(None);
        }
        let Some(previous) = create::previous_bundle(output, &self.settings.repo_name)? else {
            return Ok(None);
        };
        let current = self.git.current_refs(refs)?;
        let ref_match = create::compare_refs(self.git, &previous, &current);
        Ok(Some((previous, ref_match)))
    }

    fn prune_only(&mut self, dir: &BundleDir, keep: NonZeroUsize) -> Result<()> {
        let found = dir.find(&self.settings.repo_name)?;
        let excess = prune::select(found, keep, None);
        if excess.is_empty() {
            self.reporter.nothing_to_prune();
            return Ok(());
        }
        self.remove_excess(&excess, PruneOrigin::Flag)
    }

    /// Delete `excess`, which must not be empty, once the run is allowed to.
    fn remove_excess(&mut self, excess: &[BundleFile], origin: PruneOrigin) -> Result<()> {
        if self.settings.dry_run {
            self.reporter.would_delete(excess);
            return Ok(());
        }
        if self.settings.yes {
            return self.delete_bundles(excess);
        }
        if let Some(reason) = self.prompt_unavailable() {
            return self.prune_without_prompt(excess.len(), origin, reason);
        }
        if !confirm_deletion(excess)? {
            self.reporter.nothing_deleted();
            return Ok(());
        }
        self.delete_bundles(excess)
    }

    fn prompt_unavailable(&self) -> Option<PromptUnavailable> {
        if self.settings.json {
            return Some(PromptUnavailable::Json);
        }
        if !io::stdin().is_terminal() {
            return Some(PromptUnavailable::NoTerminal);
        }
        None
    }

    /// Skip a prune that only config asked for, so a scheduled run still succeeds. Refuse one
    /// asked for on the command line, since skipping it would hide that the flag did nothing.
    fn prune_without_prompt(
        &mut self,
        count: usize,
        origin: PruneOrigin,
        reason: PromptUnavailable,
    ) -> Result<()> {
        if origin == PruneOrigin::Flag {
            bail!(
                "pruning would delete {}, but {reason}; pass --yes to delete without asking",
                output::count_bundles(count)
            );
        }
        self.reporter.prune_skipped();
        if self.settings.warnings {
            self.reporter
                .warnings(&[Warning::PruneSkipped { count, reason }]);
        }
        Ok(())
    }

    fn delete_bundles(&mut self, excess: &[BundleFile]) -> Result<()> {
        for bundle in excess {
            prune::delete(bundle)?;
            self.reporter.deleted(&bundle.path);
        }
        Ok(())
    }
}

/// Ask on the terminal. The list and the question go straight to stderr, since only a run with
/// someone there to answer ever gets here.
fn confirm_deletion(excess: &[BundleFile]) -> Result<bool> {
    let count = output::count_bundles(excess.len());
    output::deletion_candidates(excess);
    prompt::confirm(
        &output::question(&format!("Delete {count}?")),
        io::stdin().lock(),
        io::stderr(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::config::Defaults;
    use crate::settings::Locations;
    use crate::test_support::{run_with_identity, test_repo};
    use clap::Parser;

    /// Keep every result instead of printing it.
    #[derive(Default)]
    struct Recorder {
        bundles: Vec<BundleOutcome>,
        warnings: Vec<Warning>,
        listed: Vec<ListedBundle>,
    }

    impl Reporter for Recorder {
        fn bundle(&mut self, outcome: &BundleOutcome) {
            self.bundles.push(outcome.clone());
        }

        fn warnings(&mut self, warnings: &[Warning]) {
            self.warnings.extend_from_slice(warnings);
        }

        fn nothing_to_prune(&mut self) {}

        fn would_delete(&mut self, _bundles: &[BundleFile]) {}

        fn deleted(&mut self, _path: &Path) {}

        fn nothing_deleted(&mut self) {}

        fn list(&mut self, _output: &Output, bundles: &[ListedBundle]) {
            self.listed.extend_from_slice(bundles);
        }
    }

    fn settings_for(git: &Git, flags: &[&str]) -> Settings {
        let args = Cli::try_parse_from(["repobundle"].iter().chain(flags))
            .unwrap()
            .bundle;
        let locations = Locations {
            cwd: git.repo_dir(),
            repo_root: git.repo_dir(),
        };
        Settings::resolve(&args, Defaults::default(), &locations).unwrap()
    }

    fn run_with(git: &Git, flags: &[&str]) -> Recorder {
        let settings = settings_for(git, flags);
        let mut recorder = Recorder::default();
        Runner::new(git, &settings, &mut recorder).run().unwrap();
        recorder
    }

    #[test]
    fn dry_run_reports_the_planned_bundle_without_writing_it() {
        let (_dir, git) = test_repo();
        let recorder = run_with(&git, &["--dry-run"]);

        let [BundleOutcome::WouldCreate(path)] = recorder.bundles.as_slice() else {
            panic!("expected one planned bundle, got {:?}", recorder.bundles);
        };
        assert!(!path.exists(), "{}", path.display());
    }

    #[test]
    fn second_run_reports_the_first_bundle_as_up_to_date() {
        let (_dir, git) = test_repo();
        let first = run_with(&git, &[]);
        let second = run_with(&git, &[]);

        let [BundleOutcome::Created { path, .. }] = first.bundles.as_slice() else {
            panic!("expected one created bundle, got {:?}", first.bundles);
        };
        assert_eq!(second.bundles, [BundleOutcome::UpToDate(path.clone())]);
    }

    #[test]
    fn list_marks_older_bundles_out_of_date() {
        let (_dir, git) = test_repo();
        run_with(&git, &[]);
        run_with_identity(&git, &["commit", "-q", "--allow-empty", "-m", "second"]);
        run_with(&git, &[]);
        let recorder = run_with(&git, &["--list"]);

        let refs: Vec<RefMatch> = recorder.listed.iter().map(|listed| listed.refs).collect();
        assert_eq!(refs, [RefMatch::Current, RefMatch::OutOfDate]);
        assert!(recorder.bundles.is_empty(), "{:?}", recorder.bundles);
    }

    #[test]
    fn list_of_missing_file_is_empty() {
        let (_dir, git) = test_repo();
        let recorder = run_with(&git, &["--list", "-o", "absent.bundle"]);
        assert!(recorder.listed.is_empty(), "{:?}", recorder.listed);
    }

    #[test]
    fn warnings_go_to_the_reporter() {
        let (dir, git) = test_repo();
        std::fs::write(dir.path().join("notes.txt"), "draft").unwrap();
        let recorder = run_with(&git, &[]);

        let expected = Warning::UncommittedChanges(vec!["?? notes.txt".to_string()]);
        assert!(
            recorder.warnings.contains(&expected),
            "{:?}",
            recorder.warnings
        );
    }
}
