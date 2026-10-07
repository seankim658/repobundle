use tempfile::TempDir;

use crate::git::Git;

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

pub fn empty_repo() -> (TempDir, Git) {
    let dir = TempDir::new().unwrap();
    let git = Git::new(dir.path());
    git.run(["init", "-q", "-b", "main"]).unwrap();
    (dir, git)
}

pub fn test_repo() -> (TempDir, Git) {
    let (dir, git) = empty_repo();
    run_with_identity(&git, &["commit", "-q", "--allow-empty", "-m", "initial"]);
    (dir, git)
}

/// Run git with a fixed identity and signing off, so tests don't depend on the user's config.
pub fn run_with_identity(git: &Git, args: &[&str]) {
    git.run(IDENTITY.iter().chain(args)).unwrap();
}
