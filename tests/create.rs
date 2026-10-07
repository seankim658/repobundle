mod common;

use common::Fixture;

#[test]
fn creates_verified_bundle_in_default_directory() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&[]);

    let bundles = fixture.default_bundles();
    assert_eq!(bundles.len(), 1, "{bundles:?}");
    let file_name = bundles[0].file_name().unwrap().to_str().unwrap();
    assert!(file_name.ends_with("-myrepo.bundle"), "{file_name}");
    assert!(fixture.git.verify_bundle(&bundles[0]).is_ok());
    assert!(stdout.contains(file_name), "{stdout}");
}

#[test]
fn dry_run_reports_the_name_without_creating() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&["--dry-run"]);

    assert!(stdout.contains("-myrepo.bundle"), "{stdout}");
    assert!(!fixture.default_output_dir().exists());
}

#[test]
fn bundle_output_path_writes_that_file() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&["-o", "snapshot.bundle"]);

    let bundle = fixture.repo_dir().join("snapshot.bundle");
    assert!(fixture.git.verify_bundle(&bundle).is_ok());
}

#[test]
fn repo_without_commits_fails_without_creating() {
    let fixture = Fixture::without_commits();
    let stderr = fixture.fail(&[]);

    assert!(stderr.contains("has no commits"), "{stderr}");
    assert!(!fixture.default_output_dir().exists());
}

#[test]
fn shallow_clone_fails_without_creating() {
    let fixture = Fixture::shallow_clone();
    let stderr = fixture.fail(&[]);

    assert!(stderr.contains("shallow clone"), "{stderr}");
    assert!(!fixture.default_output_dir().exists());
}

#[test]
fn unchanged_repo_reuses_the_existing_bundle() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&[]);
    let stdout = fixture.succeed(&[]);

    assert!(stdout.contains("Up to date"), "{stdout}");
    assert_eq!(fixture.default_bundles().len(), 1);
}

#[test]
fn force_creates_even_when_unchanged() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&[]);
    let stdout = fixture.succeed(&["--force"]);

    assert!(stdout.contains("Created"), "{stdout}");
}

#[test]
fn new_commit_creates_a_second_bundle() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&[]);
    fixture.commit("second");
    fixture.succeed(&[]);

    assert_eq!(fixture.default_bundles().len(), 2);
}

#[test]
fn unchanged_file_output_is_not_rewritten() {
    let fixture = Fixture::with_commit();
    fixture.succeed(&["-o", "snapshot.bundle"]);
    let stdout = fixture.succeed(&["-o", "snapshot.bundle"]);

    assert!(stdout.contains("Up to date"), "{stdout}");
}
