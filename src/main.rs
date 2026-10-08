use std::env;
use std::io;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use tracing::debug;

use repobundle::cli::{BundleArgs, Cli, Command};
use repobundle::config;
use repobundle::git::{self, Git};
use repobundle::logging;
use repobundle::output;
use repobundle::report::TextReporter;
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
    Runner::new(&git, &settings, &mut TextReporter).run()
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
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    clap_complete::generate(shell, &mut command, name, &mut io::stdout());
}
