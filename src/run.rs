use std::io::{self, IsTerminal};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::bundles::{BundleDir, BundleFile};
use crate::create::{self, RefMatch};
use crate::git::{Git, RefSet};
use crate::output;
use crate::prompt;
use crate::prune;
use crate::report::{BundleOutcome, Reporter};
use crate::settings::{Output, Prune, PruneOrigin, Settings};
use crate::warnings::{self, Warning};

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

    /// Create a bundle and prune after it, or only prune, as the settings say.
    pub fn run(&mut self) -> Result<()> {
        let settings = self.settings;
        match &settings.output {
            Output::Directory {
                dir,
                prune: Prune::Only(keep),
            } => self.prune_only(dir, *keep),
            _ => self.create_and_prune(),
        }
    }

    /// Check the repo before creating, so the warnings describe the state that was bundled.
    fn create_and_prune(&mut self) -> Result<()> {
        let settings = self.settings;
        let mut warnings = self.repo_warnings()?;
        let outcome = self.create_bundle(&mut warnings)?;
        self.reporter.bundle(&outcome);
        warnings.extend(self.bundle_warnings(outcome.path())?);
        self.reporter.warnings(&warnings);
        let Output::Directory {
            dir,
            prune: Prune::AfterCreate { keep, origin },
        } = &settings.output
        else {
            return Ok(());
        };
        let found = dir.find(&settings.repo_name)?;
        let excess = prune::select(found, *keep, Some(outcome.path()));
        self.remove_excess(&excess, *origin)
    }

    fn repo_warnings(&self) -> Result<Vec<Warning>> {
        if !self.settings.warnings {
            return Ok(Vec::new());
        }
        warnings::repo_warnings(self.git, &self.settings.output)
    }

    fn bundle_warnings(&self, bundle: &Path) -> Result<Vec<Warning>> {
        if !self.settings.warnings {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        if let Some(limit_mb) = self.settings.max_size_mb {
            found.extend(warnings::size_warning(bundle, limit_mb)?);
        }
        found.extend(warnings::unignored_output(self.git, &self.settings.output)?);
        Ok(found)
    }

    /// Return what happened to the bundle this run points to. Add a warning when the previous
    /// bundle couldn't be read.
    fn create_bundle(&self, warnings: &mut Vec<Warning>) -> Result<BundleOutcome> {
        let settings = self.settings;
        let refs = self.git.ref_set(settings.refs)?;
        let path = create::bundle_path(self.git, &settings.output, &settings.repo_name, &refs)?;
        match self.compare_with_previous(&refs)? {
            Some((existing, RefMatch::Current)) => return Ok(BundleOutcome::UpToDate(existing)),
            Some((previous, RefMatch::Unreadable)) if settings.warnings => {
                warnings.push(Warning::UnreadableBundle(previous));
            }
            _ => {}
        }
        if settings.dry_run {
            return Ok(BundleOutcome::WouldCreate(path));
        }
        let size = create::write_bundle(self.git, &path, &refs)?;
        Ok(BundleOutcome::Created { path, size })
    }

    /// Compare the previous bundle with the current refs, unless `--force` skips the check.
    fn compare_with_previous(&self, refs: &RefSet) -> Result<Option<(PathBuf, RefMatch)>> {
        let settings = self.settings;
        if settings.force {
            return Ok(None);
        }
        let Some(previous) = create::previous_bundle(&settings.output, &settings.repo_name)? else {
            return Ok(None);
        };
        let ref_match = create::compare_refs(self.git, &previous, refs)?;
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

    /// Stay silent when nothing is over the limit, so a routine create prints only its result.
    fn remove_excess(&mut self, excess: &[BundleFile], origin: PruneOrigin) -> Result<()> {
        if excess.is_empty() {
            return Ok(());
        }
        if self.settings.dry_run {
            self.reporter.would_delete(excess);
            return Ok(());
        }
        if self.settings.yes {
            return self.delete_bundles(excess);
        }
        if !io::stdin().is_terminal() {
            return self.prune_without_terminal(excess.len(), origin);
        }
        if !confirm_deletion(excess)? {
            self.reporter.nothing_deleted();
            return Ok(());
        }
        self.delete_bundles(excess)
    }

    /// Skip a prune that only config asked for, so a scheduled run still succeeds. Refuse one
    /// asked for on the command line, since skipping it would hide that the flag did nothing.
    fn prune_without_terminal(&mut self, count: usize, origin: PruneOrigin) -> Result<()> {
        if origin == PruneOrigin::Flag {
            bail!(
                "pruning would delete {}, but there is no terminal to confirm; pass --yes to delete without asking",
                output::count_bundles(count)
            );
        }
        if self.settings.warnings {
            self.reporter.warnings(&[Warning::PruneSkipped(count)]);
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
    use crate::test_support::test_repo;
    use clap::Parser;

    /// Keep every result instead of printing it.
    #[derive(Default)]
    struct Recorder {
        bundles: Vec<BundleOutcome>,
        warnings: Vec<Warning>,
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
