# General Usage

**Note.** This guide assumes the binary is on your `PATH` as `repobundle`. See [Setup](./setup.md) if it isn't.

This guide walks through creating bundles, reading what repobundle prints, and getting a repo back out of a bundle. The examples use a repo at `/home/me/code/myrepo` and run from its root.

- [Creating a Bundle](#creating-a-bundle)
  - [Bundle Names](#bundle-names)
  - [When Nothing Has Changed](#when-nothing-has-changed)
- [Choosing What to Bundle](#choosing-what-to-bundle)
- [Choosing Where Bundles Go](#choosing-where-bundles-go)
  - [Writing a Single File](#writing-a-single-file)
- [Warnings](#warnings)
- [Dry Runs](#dry-runs)
- [Errors](#errors)
- [Using a Bundle](#using-a-bundle)
- [Color and Logging](#color-and-logging)
- [See Also](#see-also)

---

## Creating a Bundle

Run repobundle with no arguments from anywhere inside a repository.

```bash
repobundle
```

```
[✓] Created bundles/20261007T133512Z-3f9a2c1-myrepo.bundle (148.2 kB)
```

That one command

1. finds the repo root, so running from a subdirectory works the same as running from the top
2. checks that the repo has at least one commit and isn't a [shallow clone](#errors)
3. creates the bundle with `git bundle create`
4. checks it with `git bundle verify`
5. moves it into place and prints its path and size

Paths are printed relative to the directory you run repobundle from. A bundle outside that directory but inside your home directory prints as `~/…`, and anything else prints in full. Error messages always use the full path.

The bundle is built under a temporary name first and only renamed once it verifies. A failed run never leaves a half-written bundle behind or replaces a good one.

To bundle a repo other than the one you're in, pass its path.

```bash
repobundle ~/code/other-repo
```

### Bundle Names

The default name is `{timestamp}-{hash}-{repo}.bundle`.

| Part          | Example            | Meaning                             |
| ------------- | ------------------ | ----------------------------------- |
| `{timestamp}` | `20261007T133512Z` | When the bundle was created, in UTC |
| `{hash}`      | `3f9a2c1`          | The commit HEAD pointed to          |
| `{repo}`      | `myrepo`           | The repo directory's name           |

The timestamp is what repobundle uses to tell which bundles are newest, so names sort correctly in any file browser too.

Use `--name` to pick a different template for one run, or set `name` in a [config file](./config_file.md#name) to change it for good.

```bash
# Put the branch in the name, separated from the repo name by @
repobundle --name "{date}-{branch}@{repo}.bundle"
```

```
[✓] Created bundles/20261007-main@myrepo.bundle (148.2 kB)
```

The config file guide lists every placeholder and the rules a template has to follow.

### When Nothing Has Changed

Before creating a bundle, repobundle compares the refs in your newest existing bundle with the refs in the repo. If every branch, tag, and `HEAD` still points where it did, there's nothing new to bundle, so it reports the bundle you already have.

```
[✓] Up to date: bundles/20261007T133512Z-3f9a2c1-myrepo.bundle
```

This only looks at commits. Uncommitted changes aren't part of a bundle, so they don't make it out of date (you'll get a [warning](#warnings) about them instead).

Pass `--force` to create a new bundle anyway.

```bash
repobundle --force
```

If the newest bundle is damaged and its refs can't be read, it counts as out of date and a fresh bundle replaces it.

## Choosing What to Bundle

`--refs` picks which refs go into the bundle. All history reachable from them comes along, and every bundle includes `HEAD`.

| Value      | Contents                                                                  |
| ---------- | ------------------------------------------------------------------------- |
| `all`      | Every ref, including remote-tracking branches, tags, notes, and the stash |
| `branches` | Local branches and tags                                                   |
| `head`     | Only the checked-out branch. On a detached HEAD, only `HEAD`              |

The default is `branches`.

```bash
# Only the branch you're on
repobundle --refs head
```

The choice also affects [the up-to-date check](#when-nothing-has-changed). With `branches`, only your own branches and tags count. With `all`, fetching from a remote moves remote-tracking branches, which makes your last bundle out of date even if you haven't committed anything. With `head`, only commits on your current branch count.

## Choosing Where Bundles Go

By default bundles go into `bundles/` at the repo root. `-o` (or `--output`) picks another directory. A relative path is resolved against the directory you run from.

```bash
repobundle -o ~/bundles
```

The directory is created if it doesn't exist.

**Inside the repo.** If the output directory is inside the repo, add it to `.gitignore` or `.git/info/exclude`. Otherwise git lists your bundles as untracked files and repobundle [warns](#warnings) about it every run.

### Writing a Single File

A path ending in `.bundle` is treated as the exact file to write, not a directory.

```bash
repobundle -o snapshot.bundle
```

```
[✓] Created snapshot.bundle (148.2 kB)
```

Each run replaces that file, and the up-to-date check compares against it. There's only ever one bundle, so there's no name template and nothing to prune. `--name`, `--prune`, and `--prune-only` are errors with a file output.

```
[!] `--name` needs a directory output, but /home/me/code/myrepo/snapshot.bundle ends in `.bundle`
```

`name` and `prune` from a config file are ignored for a file output, so a configured prune count doesn't get in the way of a one-off `-o snapshot.bundle`.

## Warnings

Warnings point out things the bundle won't contain or problems it might cause. They print to stderr after the result line, each after a `[!]` badge, so scripts reading stdout only see the result. The bundle is still created.

| Warning             | When                                                    |
| ------------------- | ------------------------------------------------------- |
| Uncommitted changes | You have modified, staged, or untracked files           |
| Submodules          | The repo has a `.gitmodules` file                       |
| Git LFS             | The repo's `.gitattributes` uses `filter=lfs`           |
| Too large           | The bundle is over `max_size_mb` from a config file     |
| Output not ignored  | The output is inside the repo and git doesn't ignore it |

**Uncommitted changes.** A bundle only holds commits. Up to 10 changed paths are listed, in `git status --short` form.

```
[!] Uncommitted changes are not in the bundle
  ?? notes.txt
   M src/main.rs
```

Commit or stash them first if they should be included.

**Submodules.** The bundle records which commit each submodule points to, but not the submodule's own files.

```
[!] Submodule contents are not in the bundle, only the commits each submodule points to
```

**Git LFS.** Files stored with LFS are replaced in history by small pointer files, and the bundle only has the pointers.

```
[!] Git LFS file contents are not in the bundle, only their pointer files
```

**Too large.** Only checked when you set `max_size_mb` in a [config file](./config_file.md#max_size_mb). It's meant to catch bundles too big to upload.

```
[!] The bundle is 42.3 MB, over the `max_size_mb` limit of 30 MB
```

**Output not ignored.**

```
[!] The output bundles is inside the repo but not ignored, so git lists it as untracked; add it to .gitignore or .git/info/exclude
```

`.git/info/exclude` works like `.gitignore` but is never committed, so it keeps the rule to your own clone.

```bash
echo "/bundles/" >> .git/info/exclude
```

Pass `--no-warnings` to silence all of them. If a run fails, only the error is printed, never warnings.

## Dry Runs

`--dry-run` shows what a run would do without writing or deleting anything.

```bash
repobundle --dry-run
```

```
[i] Would create bundles/20261007T141020Z-8be41d0-myrepo.bundle
```

The up-to-date check still runs, so a dry run on an unchanged repo prints `Up to date:` instead. Warnings about the repo are still shown. Combined with pruning, it also lists what would be deleted (see [Pruning](./pruning.md)).

## Errors

Errors print to stderr after a `[!]` badge, and repobundle exits with a non-zero status. The errors below are all caught before a bundle is written.

**No commits.** There's nothing to bundle yet.

```
[!] The repository at /home/me/code/myrepo has no commits yet; make a commit first
```

**Shallow clone.** A clone made with `--depth` is missing older history. git will build and even verify a bundle from it, but cloning from that bundle fails, so repobundle refuses up front.

```
[!] The repository at /home/me/code/myrepo is a shallow clone, so a bundle of it could not be cloned; run `git fetch --unshallow` first
```

Run `git fetch --unshallow` to download the rest of the history, then try again.

**Not a repository.** The path you passed (or the current directory) isn't inside a git repository. The message names the path and ends with git's own error.

**git not found.**

```
[!] Git was not found on the PATH; install git and try again
```

## Using a Bundle

A bundle works anywhere git expects a remote.

```bash
# Get a full working copy back
git clone 20261007T133512Z-3f9a2c1-myrepo.bundle myrepo

# Check a bundle and see which refs it holds
git bundle verify 20261007T133512Z-3f9a2c1-myrepo.bundle
git bundle list-heads 20261007T133512Z-3f9a2c1-myrepo.bundle

# Pull its branches into an existing clone without touching your own
git fetch 20261007T133512Z-3f9a2c1-myrepo.bundle 'refs/heads/*:refs/remotes/bundle/*'
```

Because the bundle includes `HEAD`, a clone from it checks out the branch you were on when it was made.

## Color and Logging

Every message starts with a badge. `[✓]` marks something done, `[i]` marks information such as a dry run, and `[!]` marks a warning (yellow) or an error (red). The badges are colored when the output goes to a terminal and plain otherwise. stdout and stderr are decided separately, so `repobundle 2> log.txt` keeps color on screen and writes plain warnings to the file. The usual variables override this.

| Variable           | Effect                                            |
| ------------------ | ------------------------------------------------- |
| `CLICOLOR_FORCE=1` | Always color, even when piped. Wins over the rest |
| `NO_COLOR=1`       | Never color                                       |
| `CLICOLOR=0`       | Never color                                       |

`--verbose` prints debug logs to stderr. They show the git version found, which config files were loaded, the settings the run ended up with, and every git command with its exit status. It's the first thing to reach for when a run doesn't do what you expected.

## See Also

- [Options](./options.md). Every flag in one place.
- [Config File](./config_file.md). Defaults for output, naming, refs, pruning, and the size limit.
- [Pruning](./pruning.md). Keeping only the newest few bundles.
