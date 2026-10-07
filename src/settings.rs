use std::ffi::OsStr;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::bundles::BundleDir;
use crate::cli::BundleArgs;
use crate::config::{Defaults, RepoName};
use crate::git::RefSelection;
use crate::naming::{DEFAULT_TEMPLATE, NameTemplate};

const DEFAULT_OUTPUT_DIR: &str = "bundles";
const BUNDLE_EXTENSION: &str = "bundle";

/// Every option after layering flags over config files over built-in defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub output: Output,
    pub refs: RefSelection,
    pub repo_name: RepoName,
    pub max_size_mb: Option<NonZeroU64>,
    pub dry_run: bool,
    pub force: bool,
    pub warnings: bool,
}

/// Where bundles live. Only a directory holds more than one bundle, so only a directory can be
/// pruned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Write a new bundle into `dir`, unless `prune` says to only prune.
    Directory { dir: BundleDir, prune: Prune },
    /// Write to exactly this path.
    File(PathBuf),
}

/// What happens to older bundles in a directory output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prune {
    /// Create a bundle and keep every older one.
    Never,
    /// Create a bundle, then keep only the newest N, counting the new one.
    AfterCreate(NonZeroUsize),
    /// Keep only the newest N without creating a bundle.
    Only(NonZeroUsize),
}

/// The directories that relative output paths resolve against.
#[derive(Debug, Clone, Copy)]
pub struct Locations<'a> {
    pub cwd: &'a Path,
    pub repo_root: &'a Path,
}

/// The config values that only apply to a directory output.
struct DirectoryDefaults {
    name: Option<NameTemplate>,
    prune: Option<NonZeroUsize>,
}

impl Settings {
    /// Expect `config.output` to be resolved already, as `config::load` returns it.
    pub fn resolve(args: &BundleArgs, config: Defaults, locations: &Locations) -> Result<Self> {
        let Defaults {
            output,
            name,
            refs,
            prune,
            max_size_mb,
            repo_name,
        } = config;
        let path = output_path(args.output.as_deref(), output, locations);
        Ok(Self {
            output: resolve_output(args, path, DirectoryDefaults { name, prune })?,
            refs: args.refs.or(refs).unwrap_or_default(),
            repo_name: resolve_repo_name(repo_name, locations.repo_root)?,
            max_size_mb,
            dry_run: args.dry_run,
            force: args.force,
            warnings: !args.no_warnings,
        })
    }
}

/// Resolve `-o` against the working directory, unlike config output, which resolves against the
/// repo root.
fn output_path(flag: Option<&Path>, config: Option<PathBuf>, locations: &Locations) -> PathBuf {
    let path = match flag {
        Some(flag) => locations.cwd.join(flag),
        None => config.unwrap_or_else(|| locations.repo_root.join(DEFAULT_OUTPUT_DIR)),
    };
    without_dot_components(&path)
}

/// Drop `.` components, so `-o .` prints as the directory itself rather than ending in `/.`.
/// Keep `..`, since removing it would change where a path through a symlink leads.
fn without_dot_components(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Treat a path ending in `.bundle` as the exact file to write, and anything else as a
/// directory. A file output drops the config's `name` and `prune`, since there is no directory
/// to name bundles in or prune.
fn resolve_output(args: &BundleArgs, path: PathBuf, defaults: DirectoryDefaults) -> Result<Output> {
    if path.extension() == Some(OsStr::new(BUNDLE_EXTENSION)) {
        reject_directory_only_flags(args, &path)?;
        return Ok(Output::File(path));
    }
    let name = args
        .name
        .clone()
        .or(defaults.name)
        .unwrap_or_else(default_template);
    Ok(Output::Directory {
        dir: BundleDir { path, name },
        prune: resolve_prune(args, defaults.prune),
    })
}

fn reject_directory_only_flags(args: &BundleArgs, file: &Path) -> Result<()> {
    if args.name.is_some() {
        bail!(
            "`--name` needs a directory output, but {} ends in `.bundle`",
            file.display()
        );
    }
    if args.prune.is_some() || args.prune_only.is_some() {
        bail!(
            "pruning needs a directory output, but {} ends in `.bundle`",
            file.display()
        );
    }
    Ok(())
}

fn resolve_prune(args: &BundleArgs, config_prune: Option<NonZeroUsize>) -> Prune {
    if let Some(count) = args.prune_only {
        return Prune::Only(count_or_config(count, config_prune));
    }
    match args.prune {
        Some(count) => Prune::AfterCreate(count_or_config(count, config_prune)),
        None if args.no_prune => Prune::Never,
        None => config_prune.map_or(Prune::Never, Prune::AfterCreate),
    }
}

/// Give a bare prune flag the config's count, or keep one bundle when config sets none.
fn count_or_config(flag: Option<NonZeroUsize>, config: Option<NonZeroUsize>) -> NonZeroUsize {
    flag.or(config).unwrap_or(NonZeroUsize::MIN)
}

fn resolve_repo_name(configured: Option<RepoName>, repo_root: &Path) -> Result<RepoName> {
    if let Some(name) = configured {
        return Ok(name);
    }
    let name = repo_root
        .file_name()
        .and_then(OsStr::to_str)
        .with_context(|| {
            format!(
                "can't name the repo after {}; set `repo_name` in a config file",
                repo_root.display()
            )
        })?;
    Ok(RepoName::try_from(name.to_string())?)
}

fn default_template() -> NameTemplate {
    DEFAULT_TEMPLATE.parse().expect("default template is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    const CWD: &str = "/work/sub";
    const REPO_ROOT: &str = "/work";

    fn parse_args(flags: &[&str]) -> BundleArgs {
        Cli::try_parse_from(["repobundle"].iter().chain(flags))
            .unwrap()
            .bundle
    }

    fn resolve(flags: &[&str], config: Defaults) -> Result<Settings> {
        let locations = Locations {
            cwd: Path::new(CWD),
            repo_root: Path::new(REPO_ROOT),
        };
        Settings::resolve(&parse_args(flags), config, &locations)
    }

    fn resolve_ok(flags: &[&str], config: Defaults) -> Settings {
        resolve(flags, config).unwrap()
    }

    fn resolve_error(flags: &[&str]) -> String {
        format!("{:#}", resolve(flags, Defaults::default()).unwrap_err())
    }

    fn count(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }

    fn repo_name(name: &str) -> RepoName {
        RepoName::try_from(name.to_string()).unwrap()
    }

    fn default_directory(name: NameTemplate) -> Output {
        Output::Directory {
            dir: BundleDir {
                path: Path::new(REPO_ROOT).join("bundles"),
                name,
            },
            prune: Prune::Never,
        }
    }

    fn output_dir(settings: &Settings) -> &Path {
        match &settings.output {
            Output::Directory { dir, .. } => &dir.path,
            Output::File(path) => panic!("expected a directory, got file {}", path.display()),
        }
    }

    fn prune_of(settings: &Settings) -> Prune {
        match &settings.output {
            Output::Directory { prune, .. } => *prune,
            Output::File(path) => panic!("expected a directory, got file {}", path.display()),
        }
    }

    // Precedence

    #[test]
    fn uses_built_in_defaults_without_flags_or_config() {
        let expected = Settings {
            output: default_directory(default_template()),
            refs: RefSelection::All,
            repo_name: repo_name("work"),
            max_size_mb: None,
            dry_run: false,
            force: false,
            warnings: true,
        };
        assert_eq!(resolve_ok(&[], Defaults::default()), expected);
    }

    #[test]
    fn config_fills_values_without_flags() {
        let config = Defaults {
            refs: Some(RefSelection::Branches),
            max_size_mb: NonZeroU64::new(30),
            repo_name: Some(repo_name("renamed")),
            ..Defaults::default()
        };
        let settings = resolve_ok(&[], config);
        assert_eq!(settings.refs, RefSelection::Branches);
        assert_eq!(settings.max_size_mb, NonZeroU64::new(30));
        assert_eq!(settings.repo_name, repo_name("renamed"));
    }

    #[test]
    fn flags_override_config() {
        let config = Defaults {
            refs: Some(RefSelection::Branches),
            name: Some("{date}-{repo}.bundle".parse().unwrap()),
            ..Defaults::default()
        };
        let settings = resolve_ok(&["--refs", "head", "--name", "{repo}.bundle"], config);
        assert_eq!(settings.refs, RefSelection::Head);
        assert_eq!(
            settings.output,
            default_directory("{repo}.bundle".parse().unwrap())
        );
    }

    // Output

    #[test]
    fn output_flag_resolves_against_working_directory() {
        let config = Defaults {
            output: Some(PathBuf::from("/elsewhere")),
            ..Defaults::default()
        };
        let settings = resolve_ok(&["-o", "out"], config);
        assert_eq!(output_dir(&settings), Path::new(CWD).join("out"));
    }

    // Compare the raw text in these two tests, since `Path` equality already ignores `.`.
    #[test]
    fn output_flag_of_dot_is_the_working_directory_itself() {
        let settings = resolve_ok(&["-o", "."], Defaults::default());
        assert_eq!(output_dir(&settings).as_os_str(), CWD);
    }

    #[test]
    fn config_output_drops_dot_components() {
        let config = Defaults {
            output: Some(PathBuf::from("/work/./bundles/.")),
            ..Defaults::default()
        };
        let settings = resolve_ok(&[], config);
        assert_eq!(output_dir(&settings).as_os_str(), "/work/bundles");
    }

    #[test]
    fn config_output_applies_without_flag() {
        let config = Defaults {
            output: Some(PathBuf::from("/elsewhere")),
            ..Defaults::default()
        };
        assert_eq!(
            output_dir(&resolve_ok(&[], config)),
            Path::new("/elsewhere")
        );
    }

    #[test]
    fn bundle_path_output_is_a_file() {
        let settings = resolve_ok(&["-o", "snapshot.bundle"], Defaults::default());
        assert_eq!(
            settings.output,
            Output::File(Path::new(CWD).join("snapshot.bundle"))
        );
    }

    #[test]
    fn rejects_name_flag_with_file_output() {
        let error = resolve_error(&["-o", "x.bundle", "--name", "{repo}.bundle"]);
        assert!(error.contains("--name"), "{error}");
    }

    #[test]
    fn rejects_prune_flags_with_file_output() {
        for flags in [
            ["-o", "x.bundle", "--prune"],
            ["-o", "x.bundle", "--prune-only=2"],
        ] {
            let error = resolve_error(&flags);
            assert!(error.contains("pruning"), "{flags:?}: {error}");
        }
    }

    #[test]
    fn file_output_drops_config_name_and_prune() {
        let config = Defaults {
            name: Some("{date}-{repo}.bundle".parse().unwrap()),
            prune: Some(count(3)),
            ..Defaults::default()
        };
        let settings = resolve_ok(&["-o", "x.bundle"], config);
        assert_eq!(settings.output, Output::File(Path::new(CWD).join("x.bundle")));
    }

    // Pruning

    #[test]
    fn resolves_prune_from_flags_and_config() {
        let cases: &[(&[&str], Option<NonZeroUsize>, Prune)] = &[
            (&[], None, Prune::Never),
            (&[], Some(count(3)), Prune::AfterCreate(count(3))),
            (&["--no-prune"], Some(count(3)), Prune::Never),
            (&["--prune"], None, Prune::AfterCreate(count(1))),
            (&["--prune"], Some(count(3)), Prune::AfterCreate(count(3))),
            (&["--prune=2"], Some(count(3)), Prune::AfterCreate(count(2))),
            (&["--prune-only"], None, Prune::Only(count(1))),
            (&["--prune-only"], Some(count(3)), Prune::Only(count(3))),
            (&["--prune-only=2"], Some(count(3)), Prune::Only(count(2))),
        ];
        for &(flags, config_prune, expected) in cases {
            let config = Defaults {
                prune: config_prune,
                ..Defaults::default()
            };
            let prune = prune_of(&resolve_ok(flags, config));
            assert_eq!(
                prune, expected,
                "{flags:?} with config prune {config_prune:?}"
            );
        }
    }

    // Repo name

    #[test]
    fn repo_root_without_a_name_needs_config() {
        let locations = Locations {
            cwd: Path::new("/"),
            repo_root: Path::new("/"),
        };
        let error =
            Settings::resolve(&parse_args(&[]), Defaults::default(), &locations).unwrap_err();
        assert!(error.to_string().contains("repo_name"), "{error}");
    }
}
