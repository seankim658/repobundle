mod common;

use std::path::Path;

use common::Fixture;

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Create `count` bundles, one per new commit, and return their file names oldest first.
fn create_bundles(fixture: &Fixture, count: usize) -> Vec<String> {
    let mut names = Vec::new();
    for index in 0..count {
        if index > 0 {
            fixture.commit(&format!("commit {index}"));
        }
        let before = fixture.default_bundles();
        fixture.succeed(&[]);
        let after = fixture.default_bundles();
        let created = after.iter().find(|path| !before.contains(path)).unwrap();
        names.push(file_name(created));
    }
    names
}

#[test]
fn prune_only_dry_run_lists_all_but_the_newest() {
    let fixture = Fixture::with_commit();
    let names = create_bundles(&fixture, 3);
    let stdout = fixture.succeed(&["--prune-only", "--dry-run"]);

    assert!(stdout.contains(&names[0]), "{stdout}");
    assert!(stdout.contains(&names[1]), "{stdout}");
    assert!(!stdout.contains(&names[2]), "{stdout}");
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn prune_dry_run_counts_the_bundle_it_would_create() {
    let fixture = Fixture::with_commit();
    let names = create_bundles(&fixture, 2);
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
    create_bundles(&fixture, 1);
    let stdout = fixture.succeed(&["--prune-only=5", "--dry-run"]);

    assert!(stdout.contains("Nothing to prune"), "{stdout}");
}

#[test]
fn prune_without_terminal_or_force_refuses_and_keeps_everything() {
    let fixture = Fixture::with_commit();
    create_bundles(&fixture, 3);
    let stderr = fixture.fail(&["--prune-only"]);

    assert!(stderr.contains("--force"), "{stderr}");
    assert_eq!(fixture.default_bundles().len(), 3);
}

#[test]
fn prune_with_nothing_to_delete_needs_no_confirmation() {
    let fixture = Fixture::with_commit();
    create_bundles(&fixture, 1);
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
fn prune_only_with_force_keeps_only_the_newest() {
    let fixture = Fixture::with_commit();
    let names = create_bundles(&fixture, 3);
    fixture.succeed(&["--prune-only", "--force"]);

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
    let names = create_bundles(&fixture, 2);
    fixture.commit("unbundled");
    let stdout = fixture.succeed(&["--prune=2", "--force"]);

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
