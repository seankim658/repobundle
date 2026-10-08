mod common;

use std::path::Path;

use common::Fixture;
use serde_json::Value;

fn parse(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

/// Create `count` bundles, one per new commit.
fn create_bundles(fixture: &Fixture, count: usize) {
    for index in 0..count {
        if index > 0 {
            fixture.commit(&format!("commit {index}"));
        }
        fixture.succeed(&[]);
    }
}

#[test]
fn created_bundle_is_reported_with_absolute_path_and_size() {
    let fixture = Fixture::with_commit();
    let report = parse(&fixture.succeed(&["--json"]));

    let bundle = &report["bundle"];
    assert_eq!(bundle["status"], "created");
    let path = bundle["path"].as_str().unwrap();
    assert!(Path::new(path).is_absolute(), "{path}");
    assert!(path.ends_with("-myrepo.bundle"), "{path}");
    assert!(bundle["size"].as_u64().unwrap() > 0, "{bundle}");
    assert_eq!(report["prune"]["status"], "none");
}

#[test]
fn reused_bundle_is_up_to_date() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&[]);
    let report = parse(&fixture.succeed(&["--json"]));

    assert_eq!(report["bundle"]["status"], "up_to_date");
    assert!(report["bundle"]["size"].is_null(), "{report}");
}

#[test]
fn warnings_go_into_the_object_instead_of_stderr() {
    let fixture = Fixture::with_commit();
    fixture.write_file("notes.txt", "draft");
    let output = fixture.run(&["--json"]);
    let report = parse(&String::from_utf8(output.stdout).unwrap());

    assert!(output.status.success());
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let kinds: Vec<&str> = report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|warning| warning["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"uncommitted_changes"), "{kinds:?}");
}

#[test]
fn config_prune_is_skipped_instead_of_asking() {
    let fixture = Fixture::with_commit();
    create_bundles(&fixture, 2);
    fixture.write_file(".repobundle.toml", "[defaults]\nprune = 1\n");
    fixture.commit("unbundled");
    let report = parse(&fixture.succeed(&["--json"]));

    assert_eq!(report["prune"]["status"], "skipped");
    let warning = report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|warning| warning["kind"] == "prune_skipped")
        .unwrap_or_else(|| panic!("no prune_skipped warning in {report}"));
    assert!(warning["message"].as_str().unwrap().contains("--json"));
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn prune_flag_without_yes_fails() {
    let fixture = Fixture::with_commit();
    create_bundles(&fixture, 2);
    let output = fixture.fail(&["--json", "--prune-only"]);

    assert!(output.contains("`--json` never asks"), "{output}");
    assert!(output.contains("--yes"), "{output}");
    assert_eq!(fixture.default_bundles().len(), 2);
}

#[test]
fn prune_with_yes_lists_the_deleted_bundles() {
    let fixture = Fixture::with_commit();
    create_bundles(&fixture, 3);
    let report = parse(&fixture.succeed(&["--json", "--prune-only", "--yes"]));

    assert!(report["bundle"].is_null(), "{report}");
    assert_eq!(report["prune"]["status"], "deleted");
    assert_eq!(report["prune"]["paths"].as_array().unwrap().len(), 2);
    assert_eq!(fixture.default_bundles().len(), 1);
}

#[test]
fn failed_run_prints_only_the_error() {
    let fixture = Fixture::without_commits();
    let output = fixture.run(&["--json"]);
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
    assert!(stderr.contains("has no commits"), "{stderr}");
}
