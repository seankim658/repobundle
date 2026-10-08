use std::env;
use std::io;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap_complete::Shell;
use tracing::debug;

use repobundle::cli::{self, BundleArgs, Cli, Command};
use repobundle::config;
use repobundle::git::{self, Git};
use repobundle::logging;
use repobundle::output;
use repobundle::report::{JsonReporter, TextReporter};
use repobundle::run::Runner;
use repobundle::settings::{Locations, Settings};

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
    if !settings.json {
        let mut reporter = TextReporter::new(settings.spinner);
        return Runner::new(&git, &settings, &mut reporter).run();
    }
    let mut reporter = JsonReporter::default();
    Runner::new(&git, &settings, &mut reporter).run()?;
    reporter.print()
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

fn print_completion(shell: Shell) {
    cli::write_completion(shell, &mut io::stdout());
}
