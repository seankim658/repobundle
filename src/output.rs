use std::env;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::OnceLock;

use colored::{Color, Colorize};

use crate::bundles::BundleFile;
use crate::size::format_size;
use crate::warnings::Warning;

static STREAM_COLORS: OnceLock<StreamColors> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy)]
struct StreamColors {
    stdout: bool,
    stderr: bool,
}

/// The environment variables that switch color on or off.
#[derive(Debug, Default)]
struct ColorEnv {
    clicolor_force: Option<String>,
    no_color: Option<String>,
    clicolor: Option<String>,
}

pub fn created(path: &Path, size: u64) {
    let label = label("Created", Color::Green, Stream::Stdout);
    println!("{label} {} ({})", path.display(), format_size(size));
}

pub fn up_to_date(path: &Path) {
    let label = label("Up to date:", Color::Green, Stream::Stdout);
    println!("{label} {}", path.display());
}

pub fn would_create(path: &Path) {
    let label = label("Would create", Color::Cyan, Stream::Stdout);
    println!("{label} {}", path.display());
}

/// Print to stderr, so warnings never mix into the output a script reads.
pub fn warnings(warnings: &[Warning]) {
    let label = label("warning:", Color::Yellow, Stream::Stderr);
    for warning in warnings {
        eprintln!("{label} {warning}");
    }
}

pub fn nothing_to_prune() {
    println!("Nothing to prune");
}

pub fn would_delete(bundles: &[BundleFile]) {
    let label = label("Would delete", Color::Cyan, Stream::Stdout);
    for bundle in bundles {
        println!("{label} {}", bundle.path.display());
    }
}

/// List the bundles a confirmation prompt is about to ask about. Use stderr, like the prompt.
pub fn deletion_candidates(bundles: &[BundleFile]) {
    for bundle in bundles {
        eprintln!("  {}", bundle.path.display());
    }
}

pub fn nothing_deleted() {
    println!("Nothing deleted");
}

pub fn deleted(path: &Path) {
    let label = label("Deleted", Color::Yellow, Stream::Stdout);
    println!("{label} {}", path.display());
}

pub fn error(error: &anyhow::Error) {
    let label = label("error:", Color::Red, Stream::Stderr);
    eprintln!("{label} {error:#}");
}

pub fn count_bundles(count: usize) -> String {
    if count == 1 {
        return "1 bundle".to_string();
    }
    format!("{count} bundles")
}

/// Style `text` as a bold label when `stream` gets color, and leave it plain otherwise.
fn label(text: &str, color: Color, stream: Stream) -> String {
    if !stream_colors().allows(stream) {
        return text.to_string();
    }
    text.color(color).bold().to_string()
}

/// Decide once per run which streams get color. Override `colored`'s own check, which looks
/// only at stdout, so this module alone decides when to style.
fn stream_colors() -> StreamColors {
    *STREAM_COLORS.get_or_init(|| {
        colored::control::set_override(true);
        let env = ColorEnv::from_process();
        StreamColors {
            stdout: env.wants_color(io::stdout().is_terminal()),
            stderr: env.wants_color(io::stderr().is_terminal()),
        }
    })
}

impl StreamColors {
    fn allows(self, stream: Stream) -> bool {
        match stream {
            Stream::Stdout => self.stdout,
            Stream::Stderr => self.stderr,
        }
    }
}

impl ColorEnv {
    fn from_process() -> Self {
        Self {
            clicolor_force: env::var("CLICOLOR_FORCE").ok(),
            no_color: env::var("NO_COLOR").ok(),
            clicolor: env::var("CLICOLOR").ok(),
        }
    }

    /// Apply the usual conventions, highest priority first. `CLICOLOR_FORCE` forces color on,
    /// a non-empty `NO_COLOR` turns it off, `CLICOLOR=0` turns it off, and otherwise only a
    /// terminal gets color.
    fn wants_color(&self, is_terminal: bool) -> bool {
        if is_enabled(self.clicolor_force.as_deref()) {
            return true;
        }
        if self
            .no_color
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        {
            return false;
        }
        if self.clicolor.as_deref() == Some("0") {
            return false;
        }
        is_terminal
    }
}

/// Treat a variable as on when it is set to anything but empty or `0`.
fn is_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    // Color

    #[test]
    fn only_a_terminal_gets_color_by_default() {
        let env = ColorEnv::default();
        assert!(env.wants_color(true));
        assert!(!env.wants_color(false));
    }

    #[test]
    fn no_color_turns_color_off() {
        let env = ColorEnv {
            no_color: value("1"),
            ..ColorEnv::default()
        };
        assert!(!env.wants_color(true));
    }

    #[test]
    fn empty_no_color_is_ignored() {
        let env = ColorEnv {
            no_color: value(""),
            ..ColorEnv::default()
        };
        assert!(env.wants_color(true));
    }

    #[test]
    fn clicolor_zero_turns_color_off() {
        let env = ColorEnv {
            clicolor: value("0"),
            ..ColorEnv::default()
        };
        assert!(!env.wants_color(true));
    }

    #[test]
    fn clicolor_force_wins_over_no_color_and_a_missing_terminal() {
        let env = ColorEnv {
            clicolor_force: value("1"),
            no_color: value("1"),
            ..ColorEnv::default()
        };
        assert!(env.wants_color(false));
    }

    #[test]
    fn clicolor_force_set_to_zero_does_not_force() {
        let env = ColorEnv {
            clicolor_force: value("0"),
            ..ColorEnv::default()
        };
        assert!(!env.wants_color(false));
    }

    // Counts

    #[test]
    fn counts_one_bundle_in_the_singular() {
        assert_eq!(count_bundles(1), "1 bundle");
        assert_eq!(count_bundles(3), "3 bundles");
    }
}
