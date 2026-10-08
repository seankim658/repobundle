//! Keep the committed scripts in `completions/` in step with the flags. Run
//! `UPDATE_COMPLETIONS=1 cargo test --test completions` to regenerate them.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use clap_complete::{Generator, Shell};
use repobundle::cli;

const BINARY_NAME: &str = "repobundle";
const COMPLETIONS_DIR: &str = "completions";
const UPDATE_VARIABLE: &str = "UPDATE_COMPLETIONS";

#[test]
fn committed_completions_match_the_flags() {
    let update = env::var_os(UPDATE_VARIABLE).is_some();
    for shell in Shell::value_variants() {
        let path = completions_dir().join(shell.file_name(BINARY_NAME));
        let script = generate(*shell);
        if update {
            write_script(&path, &script);
        } else {
            check_script(&path, &script);
        }
    }
}

fn completions_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(COMPLETIONS_DIR)
}

fn generate(shell: Shell) -> String {
    let mut script = Vec::new();
    cli::write_completion(shell, &mut script);
    String::from_utf8(script).unwrap()
}

fn write_script(path: &Path, script: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, script).unwrap();
}

/// Compare without printing both scripts, since a diff of a whole script buries the fix.
fn check_script(path: &Path, script: &str) {
    let fix = format!("run `{UPDATE_VARIABLE}=1 cargo test --test completions` to regenerate it");
    let committed = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("can't read {} ({error}); {fix}", path.display()));
    assert!(
        committed == script,
        "{} doesn't match the current flags; {fix}",
        path.display()
    );
}
