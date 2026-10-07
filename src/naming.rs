use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_TEMPLATE: &str = "{timestamp}-{hash}-{repo}.bundle";

const BUNDLE_EXTENSION: &str = ".bundle";
const TIMESTAMP_FORMAT: &str = "%Y%m%dT%H%M%SZ";
const DATE_FORMAT: &str = "%Y%m%d";
const TIMESTAMP_GROUP: &str = "timestamp";
const DETACHED_BRANCH: &str = "detached";

static PLACEHOLDER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([^{}]*)\}").expect("placeholder pattern is valid"));

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("name template is empty")]
    Empty,
    #[error("name template has unknown placeholder `{{{0}}}`")]
    UnknownPlaceholder(String),
    #[error("name template uses placeholder `{{{0}}}` more than once")]
    DuplicatePlaceholder(String),
    #[error("name template must contain `{{repo}}`")]
    MissingRepo,
    #[error(
        "name template must separate `{{{0}}}` from `{{repo}}` with a character that `{{{0}}}` can't contain, such as `@`"
    )]
    AmbiguousRepoBoundary(String),
    #[error("name template has an unmatched `{{` or `}}`")]
    UnbalancedBrace,
    #[error("name template must not contain path separators")]
    PathSeparator,
    #[error("name template must end in `.bundle`")]
    MissingExtension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placeholder {
    Timestamp,
    Date,
    Hash,
    Repo,
    Branch,
}

impl Placeholder {
    const ALL: [Self; 5] = [
        Self::Timestamp,
        Self::Date,
        Self::Hash,
        Self::Repo,
        Self::Branch,
    ];

    fn from_name(name: &str) -> Result<Self, TemplateError> {
        Self::ALL
            .into_iter()
            .find(|placeholder| placeholder.name() == name)
            .ok_or_else(|| TemplateError::UnknownPlaceholder(name.to_string()))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Timestamp => "timestamp",
            Self::Date => "date",
            Self::Hash => "hash",
            Self::Repo => "repo",
            Self::Branch => "branch",
        }
    }

    fn render(self, values: &NameValues) -> String {
        match self {
            Self::Timestamp => values.created_at.format(TIMESTAMP_FORMAT).to_string(),
            Self::Date => values.created_at.format(DATE_FORMAT).to_string(),
            Self::Hash => values.hash.to_string(),
            Self::Repo => values.repo.to_string(),
            Self::Branch => sanitize_branch(values.branch),
        }
    }

    fn parse_pattern(self, repo: &str) -> String {
        match self {
            Self::Timestamp => format!(r"(?P<{TIMESTAMP_GROUP}>\d{{8}}T\d{{6}}Z)"),
            Self::Date => r"\d{8}".to_string(),
            Self::Hash => r"[0-9a-f]{7,40}".to_string(),
            Self::Repo => regex::escape(repo),
            Self::Branch => r"[A-Za-z0-9._-]+".to_string(),
        }
    }

    fn stops_at(self, separator: &str) -> bool {
        match self {
            Self::Timestamp | Self::Date | Self::Repo => true,
            Self::Hash => !separator.chars().all(is_hash_char),
            Self::Branch => !separator.chars().all(is_branch_char),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Placeholder(Placeholder),
}

/// A validated filename template, split into literal text and placeholders.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct NameTemplate {
    segments: Vec<Segment>,
}

/// The values substituted into a template's placeholders.
#[derive(Debug, Clone)]
pub struct NameValues<'a> {
    pub created_at: DateTime<Utc>,
    pub hash: &'a str,
    pub repo: &'a str,
    /// The checked-out branch, or `None` on a detached HEAD.
    pub branch: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBundleName {
    /// Present only when the template contains `{timestamp}`.
    pub created_at: Option<DateTime<Utc>>,
}

/// Recognizes filenames that a template produced for one specific repo.
#[derive(Debug)]
pub struct BundleNameMatcher {
    regex: Regex,
}

impl FromStr for NameTemplate {
    type Err = TemplateError;

    fn from_str(template: &str) -> Result<Self, Self::Err> {
        validate_shape(template)?;
        let name_template = Self {
            segments: split_segments(template)?,
        };
        name_template.reject_duplicate_placeholders()?;
        name_template.require_repo()?;
        name_template.reject_ambiguous_repo_boundaries()?;
        Ok(name_template)
    }
}

impl TryFrom<String> for NameTemplate {
    type Error = TemplateError;

    fn try_from(template: String) -> Result<Self, Self::Error> {
        template.parse()
    }
}

impl fmt::Display for NameTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => formatter.write_str(text)?,
                Segment::Placeholder(placeholder) => {
                    write!(formatter, "{{{}}}", placeholder.name())?
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for NameTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("NameTemplate")
            .field(&self.to_string())
            .finish()
    }
}

impl NameTemplate {
    pub fn render(&self, values: &NameValues) -> String {
        let mut name = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => name.push_str(text),
                Segment::Placeholder(placeholder) => name.push_str(&placeholder.render(values)),
            }
        }
        name
    }

    pub fn matcher(&self, repo: &str) -> Result<BundleNameMatcher, regex::Error> {
        let mut pattern = String::from("^");
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => pattern.push_str(&regex::escape(text)),
                Segment::Placeholder(placeholder) => {
                    pattern.push_str(&placeholder.parse_pattern(repo))
                }
            }
        }
        pattern.push('$');
        Ok(BundleNameMatcher {
            regex: Regex::new(&pattern)?,
        })
    }

    pub fn has_timestamp(&self) -> bool {
        self.has_placeholder(Placeholder::Timestamp)
    }

    fn has_placeholder(&self, wanted: Placeholder) -> bool {
        self.placeholders().any(|placeholder| placeholder == wanted)
    }

    fn placeholders(&self) -> impl Iterator<Item = Placeholder> + '_ {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Placeholder(placeholder) => Some(*placeholder),
            Segment::Literal(_) => None,
        })
    }

    fn reject_duplicate_placeholders(&self) -> Result<(), TemplateError> {
        let mut seen = Vec::new();
        for placeholder in self.placeholders() {
            if seen.contains(&placeholder) {
                return Err(TemplateError::DuplicatePlaceholder(
                    placeholder.name().to_string(),
                ));
            }
            seen.push(placeholder);
        }
        Ok(())
    }

    fn require_repo(&self) -> Result<(), TemplateError> {
        if self.has_placeholder(Placeholder::Repo) {
            return Ok(());
        }
        Err(TemplateError::MissingRepo)
    }

    /// Reject `{hash}` or `{branch}` next to `{repo}` unless the text between them stops the
    /// neighbor's match.
    fn reject_ambiguous_repo_boundaries(&self) -> Result<(), TemplateError> {
        let mut previous: Option<Placeholder> = None;
        let mut between: &str = "";
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => between = text.as_str(),
                Segment::Placeholder(current) => {
                    if let Some(left) = previous {
                        check_repo_boundary(left, between, *current)?;
                    }
                    previous = Some(*current);
                    between = "";
                }
            }
        }
        Ok(())
    }
}

impl BundleNameMatcher {
    /// Return `None` for any file that does not belong to this repo.
    pub fn parse(&self, file_name: &str) -> Option<ParsedBundleName> {
        let captures = self.regex.captures(file_name)?;
        let created_at = match captures.name(TIMESTAMP_GROUP) {
            Some(timestamp) => Some(parse_timestamp(timestamp.as_str())?),
            None => None,
        };
        Some(ParsedBundleName { created_at })
    }
}

/// Check the rules that apply to the template as a whole string.
fn validate_shape(template: &str) -> Result<(), TemplateError> {
    if template.is_empty() {
        return Err(TemplateError::Empty);
    }
    if template.contains(['/', '\\']) {
        return Err(TemplateError::PathSeparator);
    }
    if !template.ends_with(BUNDLE_EXTENSION) {
        return Err(TemplateError::MissingExtension);
    }
    Ok(())
}

fn check_repo_boundary(
    left: Placeholder,
    between: &str,
    right: Placeholder,
) -> Result<(), TemplateError> {
    let neighbor = match (left, right) {
        (Placeholder::Repo, other) | (other, Placeholder::Repo) => other,
        _ => return Ok(()),
    };
    if neighbor.stops_at(between) {
        return Ok(());
    }
    Err(TemplateError::AmbiguousRepoBoundary(
        neighbor.name().to_string(),
    ))
}

fn split_segments(template: &str) -> Result<Vec<Segment>, TemplateError> {
    let mut segments = Vec::new();
    let mut literal_start = 0;
    for captures in PLACEHOLDER_PATTERN.captures_iter(template) {
        let whole_match = captures.get(0).expect("group 0 always exists");
        push_literal(&mut segments, &template[literal_start..whole_match.start()])?;
        let placeholder = Placeholder::from_name(&captures[1])?;
        segments.push(Segment::Placeholder(placeholder));
        literal_start = whole_match.end();
    }
    push_literal(&mut segments, &template[literal_start..])?;
    Ok(segments)
}

/// Reject braces left over between placeholders, which means a brace was unmatched.
fn push_literal(segments: &mut Vec<Segment>, text: &str) -> Result<(), TemplateError> {
    if text.contains(['{', '}']) {
        return Err(TemplateError::UnbalancedBrace);
    }
    if !text.is_empty() {
        segments.push(Segment::Literal(text.to_string()));
    }
    Ok(())
}

/// Replace every character the `{branch}` parse pattern can't match, so names always parse back.
fn sanitize_branch(branch: Option<&str>) -> String {
    let Some(branch) = branch else {
        return DETACHED_BRANCH.to_string();
    };
    branch
        .chars()
        .map(|character| {
            if is_branch_char(character) {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn is_branch_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
}

fn is_hash_char(character: char) -> bool {
    matches!(character, '0'..='9' | 'a'..='f')
}

/// Reject timestamps that fit the pattern but aren't real times, such as month 13.
fn parse_timestamp(text: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(text, TIMESTAMP_FORMAT)
        .ok()
        .map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const DEFAULT_NAME: &str = "20261004T153012Z-a1b2c3d-myrepo.bundle";

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 4, 15, 30, 12).unwrap()
    }

    fn sample_values() -> NameValues<'static> {
        NameValues {
            created_at: sample_time(),
            hash: "a1b2c3d",
            repo: "myrepo",
            branch: Some("main"),
        }
    }

    fn template(text: &str) -> NameTemplate {
        text.parse().unwrap()
    }

    fn matcher(text: &str, repo: &str) -> BundleNameMatcher {
        template(text).matcher(repo).unwrap()
    }

    fn template_error(text: &str) -> TemplateError {
        text.parse::<NameTemplate>().unwrap_err()
    }

    fn render_branch(branch: Option<&'static str>) -> String {
        let values = NameValues {
            branch,
            ..sample_values()
        };
        template("{repo}-{date}-{branch}.bundle").render(&values)
    }

    // Validation

    #[test]
    fn accepts_default_template() {
        assert!(DEFAULT_TEMPLATE.parse::<NameTemplate>().is_ok());
    }

    #[test]
    fn rejects_empty_template() {
        assert_eq!(template_error(""), TemplateError::Empty);
    }

    #[test]
    fn rejects_unknown_placeholder() {
        assert_eq!(
            template_error("{timestamp}-{author}.bundle"),
            TemplateError::UnknownPlaceholder("author".to_string())
        );
    }

    #[test]
    fn rejects_duplicate_placeholder() {
        assert_eq!(
            template_error("{hash}-{hash}.bundle"),
            TemplateError::DuplicatePlaceholder("hash".to_string())
        );
    }

    #[test]
    fn rejects_template_without_repo() {
        for text in [
            "{timestamp}-{hash}.bundle",
            "{branch}.bundle",
            "fixed.bundle",
        ] {
            assert_eq!(template_error(text), TemplateError::MissingRepo, "{text}");
        }
    }

    #[test]
    fn rejects_branch_or_hash_running_into_repo() {
        for (text, neighbor) in [
            ("{branch}-{repo}.bundle", "branch"),
            ("{repo}.{branch}.bundle", "branch"),
            ("{hash}{repo}.bundle", "hash"),
            ("{repo}abc{hash}.bundle", "hash"),
        ] {
            assert_eq!(
                template_error(text),
                TemplateError::AmbiguousRepoBoundary(neighbor.to_string()),
                "{text}"
            );
        }
    }

    #[test]
    fn accepts_repo_separated_from_branch_or_hash() {
        for text in [
            "{branch}@{repo}.bundle",
            "{repo}+{branch}.bundle",
            "{hash}-{repo}.bundle",
            "{date}{repo}.bundle",
            "{repo}{timestamp}.bundle",
        ] {
            assert!(text.parse::<NameTemplate>().is_ok(), "{text}");
        }
    }

    #[test]
    fn rejects_unbalanced_braces() {
        for text in ["{timestamp.bundle", "timestamp}.bundle", "{{repo}}.bundle"] {
            assert_eq!(
                template_error(text),
                TemplateError::UnbalancedBrace,
                "{text}"
            );
        }
    }

    #[test]
    fn rejects_path_separators() {
        for text in ["out/{repo}.bundle", "out\\{repo}.bundle"] {
            assert_eq!(template_error(text), TemplateError::PathSeparator, "{text}");
        }
    }

    #[test]
    fn rejects_missing_bundle_extension() {
        for text in ["{repo}", "{repo}.tar", "{repo}.bundle.bak"] {
            assert_eq!(
                template_error(text),
                TemplateError::MissingExtension,
                "{text}"
            );
        }
    }

    // Rendering

    #[test]
    fn renders_default_template() {
        let name = template(DEFAULT_TEMPLATE).render(&sample_values());
        assert_eq!(name, DEFAULT_NAME);
    }

    #[test]
    fn renders_date_and_branch() {
        let name = template("{date}-{branch}@{repo}.bundle").render(&sample_values());
        assert_eq!(name, "20261004-main@myrepo.bundle");
    }

    #[test]
    fn renders_branch_slashes_as_hyphens() {
        assert_eq!(
            render_branch(Some("feature/login")),
            "myrepo-20261004-feature-login.bundle"
        );
    }

    #[test]
    fn renders_unsupported_branch_characters_as_hyphens() {
        assert_eq!(
            render_branch(Some("fix+bug@v2")),
            "myrepo-20261004-fix-bug-v2.bundle"
        );
    }

    #[test]
    fn renders_detached_head_as_detached() {
        assert_eq!(render_branch(None), "myrepo-20261004-detached.bundle");
    }

    #[test]
    fn reports_whether_template_has_timestamp() {
        assert!(template(DEFAULT_TEMPLATE).has_timestamp());
        assert!(!template("{date}-{repo}.bundle").has_timestamp());
    }

    #[test]
    fn displays_as_the_original_template() {
        for text in [
            DEFAULT_TEMPLATE,
            "{date}-{branch}@{repo}.bundle",
            "fixed-{repo}.bundle",
        ] {
            assert_eq!(template(text).to_string(), text);
        }
    }

    // Parsing names back

    #[test]
    fn parses_rendered_name_back_to_its_timestamp() {
        let parsed = matcher(DEFAULT_TEMPLATE, "myrepo")
            .parse(DEFAULT_NAME)
            .unwrap();
        assert_eq!(parsed.created_at, Some(sample_time()));
    }

    #[test]
    fn round_trips_every_placeholder() {
        let text = "{timestamp}_{date}_{hash}_{branch}@{repo}.bundle";
        let values = NameValues {
            branch: Some("feature/login"),
            ..sample_values()
        };
        let name = template(text).render(&values);
        assert!(matcher(text, "myrepo").parse(&name).is_some(), "{name}");
    }

    #[test]
    fn parses_repo_names_containing_hyphens() {
        let values = NameValues {
            repo: "my-cool-repo",
            ..sample_values()
        };
        let name = template(DEFAULT_TEMPLATE).render(&values);
        assert!(
            matcher(DEFAULT_TEMPLATE, "my-cool-repo")
                .parse(&name)
                .is_some()
        );
    }

    #[test]
    fn parses_name_without_timestamp_as_no_creation_time() {
        let parsed = matcher("{date}-{repo}.bundle", "myrepo")
            .parse("20261004-myrepo.bundle")
            .unwrap();
        assert_eq!(parsed.created_at, None);
    }

    #[test]
    fn ignores_bundles_of_other_repos() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "repo");
        assert!(repo_matcher.parse(DEFAULT_NAME).is_none());
        assert!(
            repo_matcher
                .parse("20261004T153012Z-a1b2c3d-repo-old.bundle")
                .is_none()
        );
    }

    #[test]
    fn separated_branch_ignores_bundles_of_other_repos() {
        let repo_matcher = matcher("{branch}@{repo}.bundle", "repo");
        assert!(repo_matcher.parse("feature-x@repo.bundle").is_some());
        assert!(repo_matcher.parse("main@old-repo.bundle").is_none());
    }

    #[test]
    fn escapes_regex_characters_in_repo_name() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "my.repo");
        assert!(
            repo_matcher
                .parse("20261004T153012Z-a1b2c3d-my.repo.bundle")
                .is_some()
        );
        assert!(
            repo_matcher
                .parse("20261004T153012Z-a1b2c3d-myXrepo.bundle")
                .is_none()
        );
    }

    #[test]
    fn ignores_names_with_extra_text() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "myrepo");
        assert!(repo_matcher.parse(&format!("old-{DEFAULT_NAME}")).is_none());
        assert!(repo_matcher.parse(&format!("{DEFAULT_NAME}.bak")).is_none());
    }

    #[test]
    fn ignores_unrelated_files() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "myrepo");
        for name in ["", "notes.txt", "myrepo.bundle", ".DS_Store"] {
            assert!(repo_matcher.parse(name).is_none(), "{name}");
        }
    }

    #[test]
    fn ignores_malformed_hashes() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "myrepo");
        assert!(
            repo_matcher
                .parse("20261004T153012Z-A1B2C3D-myrepo.bundle")
                .is_none()
        );
        assert!(
            repo_matcher
                .parse("20261004T153012Z-a1b2c3-myrepo.bundle")
                .is_none()
        );
    }

    #[test]
    fn ignores_impossible_timestamps() {
        let repo_matcher = matcher(DEFAULT_TEMPLATE, "myrepo");
        assert!(
            repo_matcher
                .parse("20261399T999999Z-a1b2c3d-myrepo.bundle")
                .is_none()
        );
    }
}
