use std::env;
use std::io::{self, IsTerminal};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use tracing::debug;

use repobundle::bundles::{BundleDir, BundleFile};
use repobundle::cli::{BundleArgs, Cli, Command};
use repobundle::config;
use repobundle::create::{self, RefMatch};
use repobundle::git::{self, Git, RefSet};
use repobundle::logging;
use repobundle::output;
use repobundle::prompt;
use repobundle::prune;
use repobundle::settings::{Locations, Output, Prune, PruneOrigin, Settings};
use repobundle::warnings::{self, Warning};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::error(&error);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    logging::init(cli.bundle.verbose)?;
    match cli.command {
        Some(Command::Completion { shell }) => {
            print_completion(shell);
            Ok(())
        }
        None => run_bundle(&cli.bundle),
    }
}

fn run_bundle(args: &BundleArgs) -> Result<()> {
    git::ensure_installed()?;
    let git = Git::discover(&args.path)?;
    let settings = load_settings(args, &git)?;
    debug!(?settings, "resolved settings");
    match &settings.output {
        Output::Directory {
            dir,
            prune: Prune::Only(keep),
        } => prune_only(&settings, dir, *keep),
        _ => create_and_prune(&git, &settings),
    }
}

/// Check the repo before creating, so the warnings describe the state that was bundled.
fn create_and_prune(git: &Git, settings: &Settings) -> Result<()> {
    let mut warnings = repo_warnings(git, settings)?;
    let bundle = create_bundle(git, settings, &mut warnings)?;
    warnings.extend(bundle_warnings(git, settings, &bundle)?);
    output::warnings(&warnings);
    let Output::Directory {
        dir,
        prune: Prune::AfterCreate { keep, origin },
    } = &settings.output
    else {
        return Ok(());
    };
    let found = dir.find(&settings.repo_name)?;
    let excess = prune::select(found, *keep, Some(&bundle));
    remove_excess(settings, &excess, *origin)
}

fn repo_warnings(git: &Git, settings: &Settings) -> Result<Vec<Warning>> {
    if !settings.warnings {
        return Ok(Vec::new());
    }
    warnings::repo_warnings(git, &settings.output)
}

fn bundle_warnings(git: &Git, settings: &Settings, bundle: &Path) -> Result<Vec<Warning>> {
    if !settings.warnings {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    if let Some(limit_mb) = settings.max_size_mb {
        found.extend(warnings::size_warning(bundle, limit_mb)?);
    }
    found.extend(warnings::unignored_output(git, &settings.output)?);
    Ok(found)
}

fn load_settings(args: &BundleArgs, git: &Git) -> Result<Settings> {
    let cwd = env::current_dir().context("failed to read the current directory")?;
    let home = dirs::home_dir();
    let config = config::load(&args.path, git.repo_dir(), home.as_deref())?;
    let locations = Locations {
        cwd: &cwd,
        repo_root: git.repo_dir(),
    };
    Settings::resolve(args, config, &locations)
}

/// Return the bundle this run points to, whether it was created, reused, or only planned. Add a
/// warning when the previous bundle couldn't be read.
fn create_bundle(git: &Git, settings: &Settings, warnings: &mut Vec<Warning>) -> Result<PathBuf> {
    let refs = git.ref_set(settings.refs)?;
    let path = create::bundle_path(git, &settings.output, &settings.repo_name, &refs)?;
    match compare_with_previous(git, settings, &refs)? {
        Some((existing, RefMatch::Current)) => {
            output::up_to_date(&existing);
            return Ok(existing);
        }
        Some((previous, RefMatch::Unreadable)) if settings.warnings => {
            warnings.push(Warning::UnreadableBundle(previous));
        }
        _ => {}
    }
    if settings.dry_run {
        output::would_create(&path);
        return Ok(path);
    }
    let size = create::write_bundle(git, &path, &refs)?;
    output::created(&path, size);
    Ok(path)
}

/// Compare the previous bundle with the current refs, unless `--force` skips the check.
fn compare_with_previous(
    git: &Git,
    settings: &Settings,
    refs: &RefSet,
) -> Result<Option<(PathBuf, RefMatch)>> {
    if settings.force {
        return Ok(None);
    }
    let Some(previous) = create::previous_bundle(&settings.output, &settings.repo_name)? else {
        return Ok(None);
    };
    let ref_match = create::compare_refs(git, &previous, refs)?;
    Ok(Some((previous, ref_match)))
}

fn prune_only(settings: &Settings, dir: &BundleDir, keep: NonZeroUsize) -> Result<()> {
    let found = dir.find(&settings.repo_name)?;
    let excess = prune::select(found, keep, None);
    if excess.is_empty() {
        output::nothing_to_prune();
        return Ok(());
    }
    remove_excess(settings, &excess, PruneOrigin::Flag)
}

/// Stay silent when nothing is over the limit, so a routine create prints only its result.
fn remove_excess(settings: &Settings, excess: &[BundleFile], origin: PruneOrigin) -> Result<()> {
    if excess.is_empty() {
        return Ok(());
    }
    if settings.dry_run {
        output::would_delete(excess);
        return Ok(());
    }
    if settings.yes {
        return delete_bundles(excess);
    }
    if !io::stdin().is_terminal() {
        return prune_without_terminal(settings, excess.len(), origin);
    }
    if !confirm_deletion(excess)? {
        output::nothing_deleted();
        return Ok(());
    }
    delete_bundles(excess)
}

/// Skip a prune that only config asked for, so a scheduled run still succeeds. Refuse one asked
/// for on the command line, since skipping it would hide that the flag did nothing.
fn prune_without_terminal(settings: &Settings, count: usize, origin: PruneOrigin) -> Result<()> {
    if origin == PruneOrigin::Flag {
        bail!(
            "pruning would delete {}, but there is no terminal to confirm; pass --yes to delete without asking",
            output::count_bundles(count)
        );
    }
    if settings.warnings {
        output::warnings(&[Warning::PruneSkipped(count)]);
    }
    Ok(())
}

fn confirm_deletion(excess: &[BundleFile]) -> Result<bool> {
    let count = output::count_bundles(excess.len());
    output::deletion_candidates(excess);
    prompt::confirm(
        &output::question(&format!("Delete {count}?")),
        io::stdin().lock(),
        io::stderr(),
    )
}

fn delete_bundles(excess: &[BundleFile]) -> Result<()> {
    for bundle in excess {
        prune::delete(bundle)?;
        output::deleted(&bundle.path);
    }
    Ok(())
}

fn print_completion(shell: Shell) {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    clap_complete::generate(shell, &mut command, name, &mut io::stdout());
}
