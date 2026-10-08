mod common;

use std::fs;
use std::path::Path;

use common::Fixture;
use serde_json::Value;

const UNREADABLE_NAME: &str = "20200101T000000Z-abcdef1-myrepo.bundle";

#[test]
fn empty_directory_says_where_it_looked() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&["--list"]);

    assert!(
        stdout.contains("No bundles of this repo in bundles"),
        "{stdout}"
    );
    assert!(!fixture.default_output_dir().exists());
}

#[test]
fn bundles_are_listed_newest_first_with_their_status() {
    let fixture = Fixture::with_commit();
    let names = fixture.create_bundles(2);
    let stdout = fixture.succeed(&["--list"]);

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines[0].contains("2 bundles of this repo in bundles"),
        "{stdout}"
    );
    assert!(
        lines[1].contains(&names[1]) && lines[1].ends_with("current"),
        "{stdout}"
    );
    assert!(
        lines[2].contains(&names[0]) && lines[2].ends_with("out of date"),
        "{stdout}"
    );
}

#[test]
fn listing_creates_nothing() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(1);
    fixture.commit("unbundled");
    fixture.succeed(&["--list"]);

    assert_eq!(fixture.default_bundles().len(), 1);
}

#[test]
fn damaged_bundle_is_unreadable() {
    let fixture = Fixture::with_commit();
    fs::create_dir(fixture.default_output_dir()).unwrap();
    fs::write(fixture.default_output_dir().join(UNREADABLE_NAME), "junk").unwrap();
    let stdout = fixture.succeed(&["--list"]);

    assert!(stdout.contains(UNREADABLE_NAME), "{stdout}");
    assert!(stdout.trim_end().ends_with("unreadable"), "{stdout}");
}

#[test]
fn file_output_lists_that_one_file() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&["-o", "snapshot.bundle"]);
    let stdout = fixture.succeed(&["--list", "-o", "snapshot.bundle"]);

    assert!(stdout.contains("1 bundle at snapshot.bundle"), "{stdout}");
    assert!(stdout.trim_end().ends_with("current"), "{stdout}");
}

#[test]
fn json_listing_has_absolute_paths_times_and_statuses() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(1);
    let stdout = fixture.succeed(&["--list", "--json"]);
    let report: Value = serde_json::from_str(&stdout).unwrap();

    let bundle = &report["bundles"][0];
    assert!(
        Path::new(bundle["path"].as_str().unwrap()).is_absolute(),
        "{bundle}"
    );
    assert!(bundle["size"].as_u64().unwrap() > 0, "{bundle}");
    assert!(
        bundle["created_at"].as_str().unwrap().ends_with('Z'),
        "{bundle}"
    );
    assert_eq!(bundle["status"], "current");
}

#[test]
fn list_cannot_be_combined_with_pruning() {
    let fixture = Fixture::with_commit();
    let output = fixture.run(&["--list", "--prune"]);

    assert_eq!(output.status.code(), Some(2));
}
