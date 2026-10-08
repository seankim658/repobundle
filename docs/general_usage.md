# General Usage

**Note.** This guide assumes the binary is on your `PATH` as `repobundle`. See [Setup](./setup.md) if it isn't.

This guide walks through creating bundles, reading what repobundle prints, and getting a repo back out of a bundle. The examples use a repo at `/home/me/code/myrepo` and run from its root.

- [Creating a Bundle](#creating-a-bundle)
  - [Bundle Names](#bundle-names)
  - [When Nothing Has Changed](#when-nothing-has-changed)
- [Choosing What to Bundle](#choosing-what-to-bundle)
- [Including Uncommitted Work](#including-uncommitted-work)
- [Choosing Where Bundles Go](#choosing-where-bundles-go)
  - [Writing a Single File](#writing-a-single-file)
- [Warnings](#warnings)
- [Dry Runs](#dry-runs)
- [Listing Bundles](#listing-bundles)
- [JSON Output](#json-output)
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

While the bundle is written and verified, a spinner shows on stderr. It only appears when stderr is a terminal, never with `--json`, and is cleared before the result prints. Pass `--no-spinner` to turn it off.

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

If the newest bundle is damaged and its refs can't be read, it isn't reused. A fresh bundle is created and you get a [warning](#warnings) naming the damaged one.

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

A config file can also list refs by name, such as `refs = ["main", "v1.0"]`. See [`refs`](./config_file.md#refs).

The choice also affects [the up-to-date check](#when-nothing-has-changed). With `branches`, only your own branches and tags count. With `all`, fetching from a remote moves remote-tracking branches, which makes your last bundle out of date even if you haven't committed anything. With `head`, only commits on your current branch count.

## Including Uncommitted Work

A bundle only holds commits, so your uncommitted changes are left out and repobundle [warns](#warnings) about them. `--include-wip` adds the changes to tracked files as an extra commit under `refs/wip/repobundle`.

```bash
repobundle --include-wip
```

```
[✓] Created bundles/20261008T164512Z-a1b2c3d-myrepo.bundle (148.6 kB)
```

The commit is made with `git stash create`, which doesn't touch your working tree, index, or stash list. The `refs/wip/repobundle` ref only exists while the bundle is written, so nothing is left behind in your repo.

- Modified and staged files are included. Untracked files aren't, so `git add` any you want, and repobundle [warns](#warnings) about the rest.
- A bundle with uncommitted work is new every run, so the [up-to-date check](#when-nothing-has-changed) is skipped.
- With no changes to tracked files, you get a normal bundle and a note saying so.

```
[i] No changes to tracked files, so the bundle has no WIP ref
[✓] Up to date: bundles/20261007T133512Z-3f9a2c1-myrepo.bundle
```

A clone from the bundle checks out your last commit, not the uncommitted work. Fetch the WIP commit to get it back (see [Using a Bundle](#using-a-bundle)).

## Choosing Where Bundles Go

By default bundles go into `bundles/` at the repo root. `-o` (or `--output`) picks another directory. A relative path is resolved against the directory you run from.

```bash
repobundle -o ~/bundles
```

The directory is created if it doesn't exist.

**Inside the repo.** When repobundle creates the output directory inside the repo, it also writes a `.gitignore` there that ignores everything in the directory. Git never lists your bundles as untracked, and pruning leaves the `.gitignore` alone. A directory that already exists is left as it is, since it may hold files you want tracked. If git doesn't ignore an existing directory, repobundle [warns](#warnings) about it every run.

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
| Untracked files     | `--include-wip` is set and you have untracked files     |
| Submodules          | The repo has a `.gitmodules` file                       |
| Git LFS             | The repo's `.gitattributes` uses `filter=lfs`           |
| Too large           | The bundle is over `max_size_mb` from a config file     |
| Output not ignored  | The output is inside the repo and git doesn't ignore it |
| Unreadable bundle   | The newest existing bundle can't be read                |

**Uncommitted changes.** A bundle only holds commits. Up to 10 changed paths are listed, in `git status --short` form. When tracked files have changed, the warning points to [`--include-wip`](#including-uncommitted-work).

```
[!] Uncommitted changes are not in the bundle; pass --include-wip to add changes to tracked files
   M src/main.rs
  ?? notes.txt
```

Commit them, or pass `--include-wip`, if they should be included.

**Untracked files.** With `--include-wip`, changes to tracked files are in the bundle, so only untracked files are listed.

```
[!] Untracked files are not in the bundle, even with --include-wip
  ?? notes.txt
```

`git add` them before running if they should be included.

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

This only happens when the bundle goes into a directory that already existed, such as the repo root with a [single-file output](#writing-a-single-file), since repobundle only adds a `.gitignore` to a directory it creates. To fix it, ignore the output yourself. `.git/info/exclude` works like `.gitignore` but is never committed, so it keeps the rule to your own clone.

```bash
echo "/bundles/" >> .git/info/exclude
```

**Unreadable bundle.** The [up-to-date check](#when-nothing-has-changed) couldn't read the newest bundle, so a new one was made instead of reusing it. With a directory output, the damaged bundle is left in place until you delete it or a prune removes it. With a [single-file output](#writing-a-single-file), the new bundle replaces it.

```
[!] The bundle bundles/20261007T133512Z-3f9a2c1-myrepo.bundle can't be read, so it wasn't reused; run `git bundle verify` on it to see why
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

## Listing Bundles

`--list` shows this repo's bundles, newest first, without creating or deleting anything.

```bash
repobundle --list
```

```
[i] 3 bundles of this repo in bundles
  20261007T150211Z-c71d9e4-myrepo.bundle  149.0 kB  2 hours ago  current
  20261006T091544Z-1b0e7a2-myrepo.bundle  148.2 kB  1 day ago    out of date
  20261005T174802Z-9d3f210-myrepo.bundle  147.9 kB  2 days ago   out of date
```

The last column compares each bundle with the repo, the same way [the up-to-date check](#when-nothing-has-changed) does.

| Status        | Meaning                                                      |
| ------------- | ------------------------------------------------------------ |
| `current`     | It holds exactly the refs a new bundle would                 |
| `out of date` | Some ref has moved since it was made                         |
| `unreadable`  | Its refs can't be read, so it may be damaged                 |

The age comes from the timestamp in the name, or from the file's modification time when the [name template](./config_file.md#name) has none.

`--list` follows `-o`, `--name`, and `--refs` like any other run, so it lists the same bundles that pruning and the up-to-date check would see. With a [single-file output](#writing-a-single-file), it shows that one file.

```
[i] 1 bundle at snapshot.bundle
  snapshot.bundle  149.0 kB  just now  current
```

## JSON Output

`--json` prints the result as one JSON object on stdout instead of the usual messages, for scripts and other tools to read.

```bash
repobundle --json
```

```json
{
  "bundle": {
    "status": "created",
    "path": "/home/me/code/myrepo/bundles/20261007T150211Z-c71d9e4-myrepo.bundle",
    "size": 149012,
    "wip": false
  },
  "warnings": [
    {
      "kind": "uncommitted_changes",
      "message": "Uncommitted changes are not in the bundle\n  ?? notes.txt"
    }
  ],
  "prune": {
    "status": "none",
    "paths": []
  }
}
```

Paths are always absolute, wherever you run from.

| Field            | Contents                                                                                       |
| ---------------- | ---------------------------------------------------------------------------------------------- |
| `bundle`         | The bundle this run points to, or `null` with `--prune-only`                                    |
| `bundle.status`  | `created`, `up_to_date`, or `would_create` (a dry run)                                          |
| `bundle.size`    | The size in bytes of a bundle this run created, otherwise `null`                                |
| `bundle.wip`     | `true` when the bundle holds [uncommitted work](#including-uncommitted-work)                    |
| `warnings`       | Every [warning](#warnings), each with a `kind` and the `message` you'd see without `--json`     |
| `prune.status`   | `none`, `nothing_to_prune`, `would_delete`, `deleted`, or `skipped`                             |
| `prune.paths`    | The bundles deleted, or with `would_delete` the ones a real run would delete                    |

Match on a warning's `kind` rather than its message, since messages may be reworded. The kinds are `uncommitted_changes`, `untracked_files`, `submodules`, `lfs`, `too_large`, `unignored_output`, `unreadable_bundle`, and `prune_skipped`.

With `--list`, the object holds the listing instead.

```json
{
  "bundles": [
    {
      "path": "/home/me/code/myrepo/bundles/20261007T150211Z-c71d9e4-myrepo.bundle",
      "size": 149012,
      "created_at": "2026-10-07T15:02:11Z",
      "status": "current"
    }
  ]
}
```

`created_at` is in UTC, and `status` is `current`, `out_of_date`, or `unreadable`.

**It never asks.** A prune that needs confirmation is handled as if there were no terminal (see [Scripts and Scheduled Runs](./pruning.md#scripts-and-scheduled-runs)). Pass `--yes` to delete. A prune from a config file is skipped, with `prune.status` set to `skipped` and a `prune_skipped` warning. A `--prune` or `--prune-only` flag fails the run.

**Errors stay on stderr.** If the run fails, stdout is empty and the error prints to stderr as usual, with a non-zero exit status. Check the exit status before reading stdout.

**Copy the bundle's path.** Pipe `bundle.path` to your clipboard tool, then paste it into an upload dialog. On macOS, press Cmd+Shift+G in the file picker to paste a path.

```bash
repobundle --json | jq -r .bundle.path | pbcopy
```

On Linux, use `wl-copy` or `xclip -selection clipboard` in place of `pbcopy`.

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

# Restore the uncommitted work from a bundle made with --include-wip
git fetch 20261007T133512Z-3f9a2c1-myrepo.bundle refs/wip/repobundle
git stash apply FETCH_HEAD
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
