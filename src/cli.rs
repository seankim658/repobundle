use std::num::NonZeroUsize;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::git::RefSelection;
use crate::naming::NameTemplate;

/// Create, verify, and prune git bundles of a repository.
#[derive(Debug, Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub bundle: BundleArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print a shell completion script.
    Completion { shell: Shell },
}

#[derive(Debug, Args)]
pub struct BundleArgs {
    /// Repository to bundle, or any directory inside it.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory.
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Filename template used when the output is a directory.
    #[arg(long, value_name = "TEMPLATE")]
    pub name: Option<NameTemplate>,

    /// Choose which refs to bundle.
    #[arg(long, value_enum)]
    pub refs: Option<RefSelection>,

    /// After creating, keep only the newest N bundles of this repo.
    #[arg(
        long,
        value_name = "N",
        num_args = 0..=1,
        require_equals = true,
        conflicts_with = "prune_only"
    )]
    pub prune: Option<Option<NonZeroUsize>>,

    /// Keep only the newest N bundles of this repo without creating one.
    #[arg(long, value_name = "N", num_args = 0..=1, require_equals = true)]
    pub prune_only: Option<Option<NonZeroUsize>>,

    /// Ignore the `prune` value from config files.
    #[arg(long, conflicts_with_all = ["prune", "prune_only"])]
    pub no_prune: bool,

    /// Report what would be created and deleted without doing either.
    #[arg(long)]
    pub dry_run: bool,

    /// Create even if an identical bundle exists, and delete without asking.
    #[arg(long)]
    pub force: bool,

    /// Suppress all warnings.
    #[arg(long)]
    pub no_warnings: bool,

    /// Log every git command and its exit status.
    #[arg(long)]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    fn try_parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(["repobundle"].iter().chain(args))
    }

    fn parse(args: &[&str]) -> BundleArgs {
        try_parse(args).unwrap().bundle
    }

    fn parse_error(args: &[&str]) -> ErrorKind {
        try_parse(args).unwrap_err().kind()
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults_to_current_directory_without_pruning() {
        let args = parse(&[]);
        assert_eq!(args.path, PathBuf::from("."));
        assert_eq!(args.prune, None);
        assert_eq!(args.prune_only, None);
    }

    #[test]
    fn bare_prune_has_no_count() {
        assert_eq!(parse(&["--prune"]).prune, Some(None));
        assert_eq!(parse(&["--prune-only"]).prune_only, Some(None));
    }

    #[test]
    fn prune_takes_count_after_equals() {
        assert_eq!(parse(&["--prune=3"]).prune, Some(NonZeroUsize::new(3)));
    }

    #[test]
    fn prune_does_not_take_following_path_as_count() {
        let args = parse(&["--prune", "../repo"]);
        assert_eq!(args.prune, Some(None));
        assert_eq!(args.path, PathBuf::from("../repo"));
    }

    #[test]
    fn rejects_zero_prune_count() {
        for flag in ["--prune=0", "--prune-only=0"] {
            assert_eq!(parse_error(&[flag]), ErrorKind::ValueValidation, "{flag}");
        }
    }

    #[test]
    fn rejects_conflicting_prune_flags() {
        for args in [
            ["--prune", "--prune-only"],
            ["--prune", "--no-prune"],
            ["--prune-only", "--no-prune"],
        ] {
            assert_eq!(parse_error(&args), ErrorKind::ArgumentConflict, "{args:?}");
        }
    }

    #[test]
    fn rejects_invalid_name_template() {
        assert_eq!(
            parse_error(&["--name", "{author}.bundle"]),
            ErrorKind::ValueValidation
        );
    }

    #[test]
    fn parses_completion_subcommand() {
        let cli = try_parse(&["completion", "bash"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Completion { shell: Shell::Bash })
        ));
    }
}
