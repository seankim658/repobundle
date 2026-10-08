// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use repobundle::git::Git;
use tempfile::TempDir;

const REPO_NAME: &str = "myrepo";
const HOME_NAME: &str = "home";
const DEFAULT_OUTPUT_DIR: &str = "bundles";
const UPSTREAM_NAME: &str = "upstream";
const FORCE_COLOR_VARIABLE: &str = "CLICOLOR_FORCE";
const COLOR_VARIABLES: [&str; 3] = [FORCE_COLOR_VARIABLE, "NO_COLOR", "CLICOLOR"];

const IDENTITY: [&str; 8] = [
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.com",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "tag.gpgsign=false",
];

/// A repo named `myrepo` next to an empty home directory, run with no color variables set, so
/// the user's own config files and environment can't change the result.
pub struct Fixture {
    root: TempDir,
    pub git: Git,
}

impl Fixture {
    pub fn with_commit() -> Self {
        let fixture = Self::without_commits();
        fixture.commit("initial");
        fixture
    }

    /// Make an empty commit, so HEAD moves without touching the working tree.
    pub fn commit(&self, message: &str) {
        commit_in(&self.git, message);
    }

    pub fn without_commits() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join(HOME_NAME)).unwrap();
        let git = init_repo(root.path().join(REPO_NAME));
        Self { root, git }
    }

    /// A depth-1 clone of a repo with two commits, so its history is incomplete.
    pub fn shallow_clone() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join(HOME_NAME)).unwrap();
        let upstream = init_repo(root.path().join(UPSTREAM_NAME));
        commit_in(&upstream, "initial");
        commit_in(&upstream, "second");
        let url = format!("file://{}", upstream.repo_dir().display());
        let target = root.path().join(REPO_NAME);
        let clone_args = [
            "clone",
            "-q",
            "--depth",
            "1",
            url.as_str(),
            target.to_str().unwrap(),
        ];
        upstream.run(clone_args).unwrap();
        Self {
            root,
            git: Git::new(target),
        }
    }

    pub fn repo_dir(&self) -> &Path {
        self.git.repo_dir()
    }

    pub fn default_output_dir(&self) -> PathBuf {
        self.repo_dir().join(DEFAULT_OUTPUT_DIR)
    }

    /// Return the bundles in the default output directory, or none if it doesn't exist. Leave out
    /// the `.gitignore` that repobundle writes there.
    pub fn default_bundles(&self) -> Vec<PathBuf> {
        let dir = self.default_output_dir();
        if !dir.exists() {
            return Vec::new();
        }
        let entries = fs::read_dir(&dir).unwrap();
        let paths = entries.map(|entry| entry.unwrap().path());
        paths
            .filter(|path| path.extension() == Some(OsStr::new("bundle")))
            .collect()
    }

    /// Run repobundle in the repo and return its stdout, panicking with its stderr on failure.
    pub fn succeed(&self, args: &[&str]) -> String {
        expect_stdout(args, self.run(args))
    }

    /// Run like `succeed`, with color forced on even though stdout is not a terminal.
    pub fn succeed_with_color(&self, args: &[&str]) -> String {
        let mut command = self.command(args);
        command.env(FORCE_COLOR_VARIABLE, "1");
        expect_stdout(args, command.output().unwrap())
    }

    /// Run repobundle in the repo and return its stderr, panicking if it succeeds.
    pub fn fail(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(!output.status.success(), "{args:?} succeeded unexpectedly");
        String::from_utf8(output.stderr).unwrap()
    }

    /// Run repobundle in the repo and return its stderr, panicking on failure.
    pub fn succeed_with_stderr(&self, args: &[&str]) -> String {
        let output = self.run(args);
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{args:?} failed: {stderr}");
        stderr
    }

    /// Create `count` bundles, one per new commit, and return their file names oldest first.
    pub fn create_bundles(&self, count: usize) -> Vec<String> {
        let mut names = Vec::new();
        for index in 0..count {
            if index > 0 {
                self.commit(&format!("commit {index}"));
            }
            let before = self.default_bundles();
            self.succeed(&[]);
            let after = self.default_bundles();
            let created = after.iter().find(|path| !before.contains(path)).unwrap();
            names.push(created.file_name().unwrap().to_string_lossy().into_owned());
        }
        names
    }

    pub fn write_file(&self, name: &str, contents: &str) {
        fs::write(self.repo_dir().join(name), contents).unwrap();
    }

    /// Run repobundle in the repo and return everything it did, for checks across both streams.
    pub fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_repobundle"));
        command
            .args(args)
            .current_dir(self.repo_dir())
            .env("HOME", self.root.path().join(HOME_NAME));
        for variable in COLOR_VARIABLES {
            command.env_remove(variable);
        }
        command
    }
}

fn init_repo(dir: PathBuf) -> Git {
    fs::create_dir(&dir).unwrap();
    let git = Git::new(dir);
    git.run(["init", "-q", "-b", "main"]).unwrap();
    git
}

fn commit_in(git: &Git, message: &str) {
    let mut args: Vec<&str> = IDENTITY.to_vec();
    args.extend(["commit", "-q", "--allow-empty", "-m", message]);
    git.run(args).unwrap();
}

fn expect_stdout(args: &[&str], output: Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{args:?} failed: {stderr}");
    String::from_utf8(output.stdout).unwrap()
}
