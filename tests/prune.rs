mod common;

use std::path::Path;

use common::Fixture;

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn prune_only_dry_run_lists_all_but_the_newest() {
    let fixture = Fixture::with_commit();
    let names = fixture.create_bundles(3);
    let stdout = fixture.succeed(&["--prune-only", "--dry-run"]);

    assert!(stdout.contains(&names[0]), "{stdout}");
    assert!(stdout.contains(&names[1]), "{stdout}");
    assert!(!stdout.contains(&names[2]), "{stdout}");
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn prune_dry_run_counts_the_bundle_it_would_create() {
    let fixture = Fixture::with_commit();
    let names = fixture.create_bundles(2);
    fixture.commit("unbundled");
    let stdout = fixture.succeed(&["--prune=2", "--dry-run"]);

    assert!(stdout.contains("Would create"), "{stdout}");
    assert!(stdout.contains(&names[0]), "{stdout}");
    assert!(!stdout.contains(&names[1]), "{stdout}");
    assert_eq!(fixture.default_bundles().len(), 2);
}

#[test]
fn dry_run_with_few_bundles_has_nothing_to_prune() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(1);
    let stdout = fixture.succeed(&["--prune-only=5", "--dry-run"]);

    assert!(stdout.contains("Nothing to prune"), "{stdout}");
}

#[test]
fn prune_flag_without_terminal_or_yes_refuses_and_keeps_everything() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(3);
    for flag in ["--prune-only", "--prune"] {
        let stderr = fixture.fail(&[flag]);
        assert!(stderr.contains("--yes"), "{flag}: {stderr}");
    }
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn config_prune_without_terminal_warns_and_keeps_everything() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(2);
    fixture.write_file(".repobundle.toml", "[defaults]\nprune = 1\n");
    fixture.commit("unbundled");
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("nothing was deleted"), "{stderr}");
    assert!(stderr.contains("--yes"), "{stderr}");
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn force_does_not_approve_deletion() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(2);
    // Move HEAD so the forced bundle gets a new name. On the same commit in the same second,
    // it would replace the newest bundle instead of adding one.
    fixture.commit("unbundled");
    let stderr = fixture.fail(&["--prune", "--force"]);

    assert!(stderr.contains("--yes"), "{stderr}");
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn prune_with_nothing_to_delete_needs_no_confirmation() {
    let fixture = Fixture::with_commit();
    fixture.create_bundles(1);
    let stdout = fixture.succeed(&["--prune-only=5"]);

    assert!(stdout.contains("Nothing to prune"), "{stdout}");
}

#[test]
fn prune_after_create_with_nothing_to_delete_stays_quiet() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&["--prune=5"]);

    assert!(stdout.contains("Created"), "{stdout}");
    assert!(!stdout.contains("Nothing to prune"), "{stdout}");
}

#[test]
fn prune_only_with_yes_keeps_only_the_newest() {
    let fixture = Fixture::with_commit();
    let names = fixture.create_bundles(3);
    fixture.succeed(&["--prune-only", "--yes"]);

    let remaining: Vec<String> = fixture
        .default_bundles()
        .iter()
        .map(|path| file_name(path))
        .collect();
    assert_eq!(remaining, [names[2].clone()]);
}

#[test]
fn prune_after_create_keeps_the_new_bundle() {
    let fixture = Fixture::with_commit();
    let names = fixture.create_bundles(2);
    fixture.commit("unbundled");
    let stdout = fixture.succeed(&["--prune=2", "--yes"]);

    let mut remaining: Vec<String> = fixture
        .default_bundles()
        .iter()
        .map(|path| file_name(path))
        .collect();
    remaining.sort();
    assert!(stdout.contains("Deleted"), "{stdout}");
    assert_eq!(remaining.len(), 2, "{remaining:?}");
    assert!(remaining.contains(&names[1]), "{remaining:?}");
    assert!(!remaining.contains(&names[0]), "{remaining:?}");
}
