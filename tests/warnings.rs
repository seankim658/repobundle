mod common;

use std::fs;

use common::Fixture;

const CONFIG_FILE: &str = ".repobundle.toml";
const ONE_MB_LIMIT: &str = "[defaults]\nmax_size_mb = 1\n";
const BIG_FILE_BYTES: usize = 1_500_000;

/// Return bytes that compression can't shrink, so the bundle ends up about as large as the file.
fn incompressible_bytes(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(len);
    bytes
}

#[test]
fn uncommitted_file_is_reported_and_bundle_is_still_created() {
    let fixture = Fixture::with_commit();
    fixture.write_file("notes.txt", "draft");
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("Uncommitted changes"), "{stderr}");
    assert!(stderr.contains("notes.txt"), "{stderr}");
    assert_eq!(fixture.default_bundles().len(), 1);
}

#[test]
fn no_warnings_flag_silences_warnings() {
    let fixture = Fixture::with_commit();
    fixture.write_file("notes.txt", "draft");
    let stderr = fixture.succeed_with_stderr(&["--no-warnings"]);

    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn prune_only_does_not_check_the_repo() {
    let fixture = Fixture::with_commit();
    fixture.write_file("notes.txt", "draft");
    let stderr = fixture.succeed_with_stderr(&["--prune-only=5"]);

    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn new_output_directory_ignores_itself() {
    let fixture = Fixture::with_commit();
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.is_empty(), "{stderr}");
    assert!(fixture.default_output_dir().join(".gitignore").is_file());
    assert!(fixture.git.status_lines(None).unwrap().is_empty());
}

#[test]
fn unignored_output_is_reported_but_not_as_a_change() {
    let fixture = Fixture::with_commit();
    fs::create_dir(fixture.default_output_dir()).unwrap();
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("not ignored"), "{stderr}");
    assert!(!stderr.contains("Uncommitted"), "{stderr}");
}

#[test]
fn ignored_output_in_a_clean_repo_has_no_warnings() {
    let fixture = Fixture::with_commit();
    fixture.write_file(".gitignore", "/bundles/\n");
    fixture.git.run(["add", ".gitignore"]).unwrap();
    fixture.commit("ignore bundles");
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn submodules_are_reported() {
    let fixture = Fixture::with_commit();
    fixture.write_file(".gitmodules", "");
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("Submodule contents"), "{stderr}");
}

#[test]
fn lfs_is_reported() {
    let fixture = Fixture::with_commit();
    fixture.write_file(
        ".gitattributes",
        "*.psd filter=lfs diff=lfs merge=lfs -text\n",
    );
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("Git LFS"), "{stderr}");
}

#[test]
fn bundle_over_max_size_is_reported() {
    let fixture = Fixture::with_commit();
    let big_file = fixture.repo_dir().join("big.bin");
    fs::write(big_file, incompressible_bytes(BIG_FILE_BYTES)).unwrap();
    fixture.git.run(["add", "big.bin"]).unwrap();
    fixture.commit("add big file");
    fixture.write_file(CONFIG_FILE, ONE_MB_LIMIT);
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(stderr.contains("`max_size_mb` limit of 1 MB"), "{stderr}");
}

#[test]
fn bundle_under_max_size_is_not_reported() {
    let fixture = Fixture::with_commit();
    fixture.write_file(CONFIG_FILE, ONE_MB_LIMIT);
    let stderr = fixture.succeed_with_stderr(&[]);

    assert!(!stderr.contains("max_size_mb"), "{stderr}");
}

#[test]
fn unreadable_previous_bundle_is_reported_and_replaced() {
    let fixture = Fixture::with_commit();
    fixture.write_file("snapshot.bundle", "not a bundle");
    let stderr = fixture.succeed_with_stderr(&["-o", "snapshot.bundle"]);

    let expected = "The bundle snapshot.bundle can't be read, so it wasn't reused";
    assert!(stderr.contains(expected), "{stderr}");
    let bundle = fixture.repo_dir().join("snapshot.bundle");
    assert!(fixture.git.verify_bundle(&bundle).is_ok());
}
