mod common;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use common::Fixture;
use repobundle::git::{Git, WIP_REF};
use serde_json::Value;
use tempfile::TempDir;

const TRACKED_FILE: &str = "tracked.txt";

/// A repo with one committed file that has changed in the working tree.
fn fixture_with_tracked_change() -> Fixture {
    let fixture = Fixture::with_commit();
    fixture.write_file(TRACKED_FILE, "committed");
    fixture.git.run(["add", TRACKED_FILE]).unwrap();
    fixture.commit("add tracked file");
    fixture.write_file(TRACKED_FILE, "uncommitted");
    fixture
}

fn bundle_with_wip(fixture: &Fixture) -> PathBuf {
    fixture.succeed(&["--include-wip", "-o", "wip.bundle"]);
    fixture.repo_dir().join("wip.bundle")
}

fn clone_bundle(fixture: &Fixture, bundle: &Path, target: &Path) -> Git {
    let clone_args = [
        OsStr::new("clone"),
        OsStr::new("-q"),
        bundle.as_os_str(),
        target.as_os_str(),
    ];
    fixture.git.run(clone_args).unwrap();
    Git::new(target)
}

#[test]
fn wip_can_be_fetched_from_the_bundle() {
    let fixture = fixture_with_tracked_change();
    let bundle = bundle_with_wip(&fixture);
    let target = TempDir::new().unwrap();
    let clone = clone_bundle(&fixture, &bundle, target.path());

    let fetch_args = [
        OsStr::new("fetch"),
        OsStr::new("-q"),
        bundle.as_os_str(),
        OsStr::new(WIP_REF),
    ];
    clone.run(fetch_args).unwrap();
    let fetched = clone.run(["show", "FETCH_HEAD:tracked.txt"]).unwrap();
    assert_eq!(fetched, "uncommitted");
}

#[test]
fn wip_ref_is_removed_from_the_repo() {
    let fixture = fixture_with_tracked_change();
    bundle_with_wip(&fixture);

    let lookup = fixture
        .git
        .run(["rev-parse", "--verify", "--quiet", WIP_REF]);
    assert!(lookup.is_err(), "{lookup:?}");
}

#[test]
fn clean_tree_says_there_is_nothing_to_include() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&["--include-wip", "-o", "../clean.bundle"]);

    assert!(stdout.contains("No changes to tracked files"), "{stdout}");
}

#[test]
fn tracked_changes_without_the_flag_suggest_it() {
    let fixture = fixture_with_tracked_change();
    let stderr = fixture.succeed_with_stderr(&["-o", "../plain.bundle"]);

    assert!(stderr.contains("--include-wip"), "{stderr}");
}

#[test]
fn with_the_flag_only_untracked_files_are_warned_about() {
    let fixture = fixture_with_tracked_change();
    fixture.write_file("notes.txt", "draft");
    let stderr = fixture.succeed_with_stderr(&["--include-wip", "-o", "../wip.bundle"]);

    assert!(
        stderr.contains("Untracked files are not in the bundle"),
        "{stderr}"
    );
    assert!(stderr.contains("notes.txt"), "{stderr}");
    assert!(!stderr.contains(TRACKED_FILE), "{stderr}");
}

#[test]
fn json_marks_a_bundle_with_wip() {
    let fixture = fixture_with_tracked_change();
    let stdout = fixture.succeed(&["--include-wip", "--json", "-o", "../wip.bundle"]);
    let report: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(report["bundle"]["wip"], true);
}
