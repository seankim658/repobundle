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

/// The bracketed marker that starts each message and says what kind it is.
#[derive(Debug, Clone, Copy)]
enum Badge {
    Success,
    Info,
    Warning,
    Error,
}

pub fn created(path: &Path, size: u64) {
    let badge = badge(Badge::Success);
    println!("{badge} Created {} ({})", path.display(), format_size(size));
}

pub fn up_to_date(path: &Path) {
    println!("{} Up to date: {}", badge(Badge::Success), path.display());
}

pub fn would_create(path: &Path) {
    println!("{} Would create {}", badge(Badge::Info), path.display());
}

/// Print to stderr, so warnings never mix into the output a script reads.
pub fn warnings(warnings: &[Warning]) {
    let badge = badge(Badge::Warning);
    for warning in warnings {
        eprintln!("{badge} {}", capitalize(&warning.to_string()));
    }
}

pub fn nothing_to_prune() {
    println!("{} Nothing to prune", badge(Badge::Info));
}

pub fn would_delete(bundles: &[BundleFile]) {
    let badge = badge(Badge::Info);
    for bundle in bundles {
        println!("{badge} Would delete {}", bundle.path.display());
    }
}

/// List the bundles a confirmation prompt is about to ask about. Use stderr, like the prompt.
pub fn deletion_candidates(bundles: &[BundleFile]) {
    for bundle in bundles {
        eprintln!("  - {}", bundle.path.display());
    }
}

/// Style a confirmation question in bold when stderr, where the prompt goes, gets color.
pub fn question(text: &str) -> String {
    if !stream_colors().allows(Stream::Stderr) {
        return text.to_string();
    }
    text.bold().to_string()
}

pub fn nothing_deleted() {
    println!("{} Nothing deleted", badge(Badge::Info));
}

pub fn deleted(path: &Path) {
    println!("{} Deleted {}", badge(Badge::Success), path.display());
}

pub fn error(error: &anyhow::Error) {
    eprintln!(
        "{} {}",
        badge(Badge::Error),
        capitalize(&format!("{error:#}"))
    );
}

pub fn count_bundles(count: usize) -> String {
    if count == 1 {
        return "1 bundle".to_string();
    }
    format!("{count} bundles")
}

fn badge(kind: Badge) -> String {
    render_badge(kind, stream_colors().allows(kind.stream()))
}

/// Color only the symbol, and bold the brackets around it, when `colored` is set.
fn render_badge(kind: Badge, colored: bool) -> String {
    let symbol = kind.symbol();
    if !colored {
        return format!("[{symbol}]");
    }
    let symbol = symbol.color(kind.color()).bold();
    format!("{}{symbol}{}", "[".bold(), "]".bold())
}

/// Uppercase the first letter, since each message reads as a sentence after its badge.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Badge {
    fn symbol(self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Info => "i",
            Self::Warning | Self::Error => "!",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Success => Color::Green,
            Self::Info => Color::Blue,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
        }
    }

    /// Send warnings and errors to stderr, and everything else to stdout.
    fn stream(self) -> Stream {
        match self {
            Self::Success | Self::Info => Stream::Stdout,
            Self::Warning | Self::Error => Stream::Stderr,
        }
    }
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

    // Badges

    #[test]
    fn plain_badges_keep_their_brackets() {
        assert_eq!(render_badge(Badge::Success, false), "[✓]");
        assert_eq!(render_badge(Badge::Info, false), "[i]");
        assert_eq!(render_badge(Badge::Warning, false), "[!]");
        assert_eq!(render_badge(Badge::Error, false), "[!]");
    }

    #[test]
    fn capitalizes_only_the_first_letter() {
        assert_eq!(capitalize("failed to read `x`"), "Failed to read `x`");
        assert_eq!(capitalize("`--name` is invalid"), "`--name` is invalid");
        assert_eq!(capitalize(""), "");
    }

    // Counts

    #[test]
    fn counts_one_bundle_in_the_singular() {
        assert_eq!(count_bundles(1), "1 bundle");
        assert_eq!(count_bundles(3), "3 bundles");
    }
}
