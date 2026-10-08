mod common;

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use common::Fixture;
use repobundle::git::Git;
use tempfile::TempDir;

/// A repo on `main` with another branch, a tag, and a remote-tracking ref, so each `--refs`
/// value bundles a different set.
fn fixture_with_every_ref_kind() -> Fixture {
    let fixture = Fixture::with_commit();
    fixture.git.run(["branch", "feature"]).unwrap();
    fixture.git.run(["tag", "v1"]).unwrap();
    fixture
        .git
        .run(["update-ref", "refs/remotes/origin/main", "HEAD"])
        .unwrap();
    fixture
}

/// Bundle with `--refs <refs>` into its own file and return the file's path.
fn bundle_with(fixture: &Fixture, refs: &str) -> PathBuf {
    let file_name = format!("{refs}.bundle");
    fixture.succeed(&["--refs", refs, "-o", &file_name]);
    fixture.repo_dir().join(file_name)
}

fn ref_names(fixture: &Fixture, bundle: &Path) -> BTreeSet<String> {
    let tips = fixture.git.bundle_refs(bundle).unwrap();
    tips.into_iter().map(|tip| tip.name).collect()
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| name.to_string()).collect()
}

#[test]
fn all_bundles_every_kind_of_ref() {
    let fixture = fixture_with_every_ref_kind();
    let bundle = bundle_with(&fixture, "all");

    let expected = names(&[
        "HEAD",
        "refs/heads/feature",
        "refs/heads/main",
        "refs/remotes/origin/main",
        "refs/tags/v1",
    ]);
    assert_eq!(ref_names(&fixture, &bundle), expected);
}

#[test]
fn default_refs_value_is_branches() {
    let fixture = fixture_with_every_ref_kind();
    fixture.succeed(&["-o", "default.bundle"]);
    let default = fixture.repo_dir().join("default.bundle");
    let branches = bundle_with(&fixture, "branches");

    assert_eq!(
        ref_names(&fixture, &default),
        ref_names(&fixture, &branches)
    );
}

#[test]
fn branches_leaves_out_remote_tracking_refs() {
    let fixture = fixture_with_every_ref_kind();
    let bundle = bundle_with(&fixture, "branches");

    let expected = names(&[
        "HEAD",
        "refs/heads/feature",
        "refs/heads/main",
        "refs/tags/v1",
    ]);
    assert_eq!(ref_names(&fixture, &bundle), expected);
}

#[test]
fn head_bundles_only_the_checked_out_branch() {
    let fixture = fixture_with_every_ref_kind();
    let bundle = bundle_with(&fixture, "head");

    assert_eq!(
        ref_names(&fixture, &bundle),
        names(&["HEAD", "refs/heads/main"])
    );
}

#[test]
fn head_on_a_detached_head_bundles_only_head() {
    let fixture = fixture_with_every_ref_kind();
    fixture.git.run(["checkout", "-q", "--detach"]).unwrap();
    let bundle = bundle_with(&fixture, "head");

    assert_eq!(ref_names(&fixture, &bundle), names(&["HEAD"]));
}

#[test]
fn every_refs_value_clones_to_the_same_head() {
    let fixture = fixture_with_every_ref_kind();
    let head = fixture.git.run(["rev-parse", "HEAD"]).unwrap();

    for refs in ["all", "branches", "head"] {
        let bundle = bundle_with(&fixture, refs);
        let target = TempDir::new().unwrap();
        let clone_args = [
            OsStr::new("clone"),
            OsStr::new("-q"),
            bundle.as_os_str(),
            target.path().as_os_str(),
        ];
        fixture.git.run(clone_args).unwrap();

        let cloned_head = Git::new(target.path()).run(["rev-parse", "HEAD"]).unwrap();
        assert_eq!(cloned_head, head, "--refs {refs}");
    }
}
