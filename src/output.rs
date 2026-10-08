use std::env;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use colored::{Color, Colorize};

use crate::bundles::BundleFile;
use crate::create::RefMatch;
use crate::report::ListedBundle;
use crate::settings::Output;
use crate::size::format_size;
use crate::warnings::Warning;

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

static STREAM_COLORS: OnceLock<StreamColors> = OnceLock::new();
static PATH_BASES: OnceLock<PathBases> = OnceLock::new();

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

/// The directories that printed paths are shortened against. Either can be missing, and then
/// paths just print in full.
#[derive(Debug, Default)]
struct PathBases {
    cwd: Option<PathBuf>,
    home: Option<PathBuf>,
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
    println!(
        "{badge} Created {} ({})",
        display_path(path),
        format_size(size)
    );
}

pub fn up_to_date(path: &Path) {
    println!(
        "{} Up to date: {}",
        badge(Badge::Success),
        display_path(path)
    );
}

pub fn would_create(path: &Path) {
    println!("{} Would create {}", badge(Badge::Info), display_path(path));
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
        println!("{badge} Would delete {}", display_path(&bundle.path));
    }
}

/// List the bundles a confirmation prompt is about to ask about, under a heading of their own
/// so the list doesn't read as part of the line above. Use stderr, like the prompt.
pub fn deletion_candidates(bundles: &[BundleFile]) {
    let badge = render_badge(Badge::Info, stream_colors().allows(Stream::Stderr));
    eprintln!("{badge} Bundles over the prune limit");
    for bundle in bundles {
        eprintln!("  - {}", display_path(&bundle.path));
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
    println!("{} Deleted {}", badge(Badge::Success), display_path(path));
}

/// Print a heading, then one aligned row per bundle with its name, size, age, and whether it
/// holds the current refs.
pub fn listing(output: &Output, bundles: &[ListedBundle]) {
    let badge = badge(Badge::Info);
    println!("{badge} {}", listing_heading(output, bundles.len()));
    let now = Utc::now();
    let rows: Vec<[String; 4]> = bundles.iter().map(|listed| list_row(listed, now)).collect();
    for line in aligned(&rows) {
        println!("  {line}");
    }
}

fn listing_heading(output: &Output, count: usize) -> String {
    match output {
        Output::Directory(dir) if count == 0 => {
            format!("No bundles of this repo in {}", display_path(&dir.path))
        }
        Output::Directory(dir) => format!(
            "{} of this repo in {}",
            count_bundles(count),
            display_path(&dir.path)
        ),
        Output::File(path) if count == 0 => format!("No bundle at {}", display_path(path)),
        Output::File(path) => format!("{} at {}", count_bundles(count), display_path(path)),
    }
}

fn list_row(listed: &ListedBundle, now: DateTime<Utc>) -> [String; 4] {
    let bundle = &listed.bundle;
    let name = bundle.path.file_name().unwrap_or(bundle.path.as_os_str());
    let age = (now - bundle.created()).num_seconds();
    [
        name.to_string_lossy().into_owned(),
        format_size(bundle.size),
        format_age(age),
        ref_status(listed.refs).to_string(),
    ]
}

/// Pad every column but the last to its widest cell. Right-align sizes, so their units line up.
fn aligned(rows: &[[String; 4]]) -> Vec<String> {
    let width = |column: usize| {
        let cells = rows.iter().map(|row| row[column].chars().count());
        cells.max().unwrap_or(0)
    };
    let (name_width, size_width, age_width) = (width(0), width(1), width(2));
    rows.iter()
        .map(|[name, size, age, status]| {
            format!("{name:<name_width$}  {size:>size_width$}  {age:<age_width$}  {status}")
        })
        .collect()
}

/// Describe an age in the largest whole unit, up to days. Treat a time in the future, from a
/// clock that moved backward, as just now.
fn format_age(seconds: i64) -> String {
    if seconds < SECONDS_PER_MINUTE {
        return "just now".to_string();
    }
    let (count, unit) = if seconds < SECONDS_PER_HOUR {
        (seconds / SECONDS_PER_MINUTE, "minute")
    } else if seconds < SECONDS_PER_DAY {
        (seconds / SECONDS_PER_HOUR, "hour")
    } else {
        (seconds / SECONDS_PER_DAY, "day")
    };
    if count == 1 {
        return format!("1 {unit} ago");
    }
    format!("{count} {unit}s ago")
}

fn ref_status(refs: RefMatch) -> &'static str {
    match refs {
        RefMatch::Current => "current",
        RefMatch::OutOfDate => "out of date",
        RefMatch::Unreadable => "unreadable",
    }
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

/// Shorten `path` the way every message prints it. Errors and logs print full paths instead.
pub fn display_path(path: &Path) -> String {
    PATH_BASES
        .get_or_init(PathBases::from_process)
        .shorten(path)
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
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl PathBases {
    fn from_process() -> Self {
        Self {
            cwd: env::current_dir().ok(),
            home: dirs::home_dir(),
        }
    }

    /// Show `path` relative to the working directory when it's inside it, under `~` when it's
    /// inside the home directory, and in full otherwise. Every form still works in the same shell.
    fn shorten(&self, path: &Path) -> String {
        if let Some(relative) = strict_suffix(path, self.cwd.as_deref()) {
            return relative.display().to_string();
        }
        if let Some(relative) = strict_suffix(path, self.home.as_deref()) {
            return Path::new("~").join(relative).display().to_string();
        }
        path.display().to_string()
    }
}

/// Return what follows `base` in `path`, when `path` is strictly inside `base`.
fn strict_suffix<'a>(path: &'a Path, base: Option<&Path>) -> Option<&'a Path> {
    let relative = path.strip_prefix(base?).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    Some(relative)
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

    // Paths

    fn bases() -> PathBases {
        PathBases {
            cwd: Some(PathBuf::from("/home/me/code/myrepo")),
            home: Some(PathBuf::from("/home/me")),
        }
    }

    #[test]
    fn path_inside_working_directory_is_relative() {
        let path = Path::new("/home/me/code/myrepo/bundles/a.bundle");
        assert_eq!(bases().shorten(path), "bundles/a.bundle");
    }

    #[test]
    fn path_elsewhere_in_home_starts_with_tilde() {
        let path = Path::new("/home/me/bundles/a.bundle");
        assert_eq!(bases().shorten(path), "~/bundles/a.bundle");
    }

    #[test]
    fn path_outside_home_prints_in_full() {
        let path = Path::new("/mnt/bundles/a.bundle");
        assert_eq!(bases().shorten(path), "/mnt/bundles/a.bundle");
        assert_eq!(PathBases::default().shorten(path), "/mnt/bundles/a.bundle");
    }

    #[test]
    fn working_directory_itself_is_never_empty() {
        let path = Path::new("/home/me/code/myrepo");
        assert_eq!(bases().shorten(path), "~/code/myrepo");
    }

    // Listing

    #[test]
    fn ages_use_the_largest_whole_unit() {
        assert_eq!(format_age(-5), "just now");
        assert_eq!(format_age(59), "just now");
        assert_eq!(format_age(60), "1 minute ago");
        assert_eq!(format_age(3 * SECONDS_PER_HOUR - 1), "2 hours ago");
        assert_eq!(format_age(SECONDS_PER_DAY), "1 day ago");
        assert_eq!(format_age(40 * SECONDS_PER_DAY), "40 days ago");
    }

    #[test]
    fn listing_columns_line_up() {
        let rows = [
            [
                "long-name.bundle".to_string(),
                "1.2 MB".to_string(),
                "2 hours ago".to_string(),
                "current".to_string(),
            ],
            [
                "a.bundle".to_string(),
                "149.0 kB".to_string(),
                "1 day ago".to_string(),
                "out of date".to_string(),
            ],
        ];
        assert_eq!(
            aligned(&rows),
            [
                "long-name.bundle    1.2 MB  2 hours ago  current",
                "a.bundle          149.0 kB  1 day ago    out of date",
            ]
        );
    }

    #[test]
    fn empty_listings_say_where_they_looked() {
        let file = Output::File(PathBuf::from("/mnt/x.bundle"));
        assert_eq!(listing_heading(&file, 0), "No bundle at /mnt/x.bundle");
        assert_eq!(listing_heading(&file, 1), "1 bundle at /mnt/x.bundle");
    }

    // Counts

    #[test]
    fn counts_one_bundle_in_the_singular() {
        assert_eq!(count_bundles(1), "1 bundle");
        assert_eq!(count_bundles(3), "3 bundles");
    }
}
