# Config File

repobundle reads up to two optional config files and uses their values as defaults for every run. If neither exists, it falls back to its built-in defaults.

A ready-to-edit sample lives at [`repobundle.example.toml`](../repobundle.example.toml) in the repo root. Copy it to one of the locations below and adjust. Every value is optional.

- [Global vs. Project Scope](#global-vs-project-scope)
  - [How the Project File is Found](#how-the-project-file-is-found)
  - [Precedence](#precedence)
- [`[defaults]`](#defaults)
  - [`output`](#output)
  - [`name`](#name)
  - [`refs`](#refs)
  - [`prune`](#prune)
  - [`max_size_mb`](#max_size_mb)
  - [`repo_name`](#repo_name)
- [Errors](#errors)
- [See Also](#see-also)

---

## Global vs. Project Scope

Config files live in one of two places.

| Scope       | File                           | Best for                          |
| ----------- | ------------------------------ | --------------------------------- |
| **Global**  | `~/.repobundle.toml`           | Defaults you want in every repo   |
| **Project** | `.repobundle.toml` in the repo | Defaults specific to one codebase |

Both files use the same schema, a single `[defaults]` table.

Good candidates for the **global** file are preferences that don't depend on the repo, such as `max_size_mb` for the upload limit of the chat you use most. Good candidates for the **project** file are `repo_name` and a `prune` count that suits how often you bundle that repo.

### How the Project File is Found

repobundle walks up from the `PATH` argument (the current directory by default) looking for a `.repobundle.toml`, and uses the closest one it finds. The search stops at the repo root, so a file above the repo is never read.

Your home `~/.repobundle.toml` is never picked up as a project file, even when your home directory is itself a git repository.

### Precedence

Values are resolved key by key, highest first.

1. A command-line flag
2. The project file
3. The global file
4. The built-in default

A key you don't set in the project file still comes from the global file. With these two files

```toml
# ~/.repobundle.toml
[defaults]
prune = 5
refs = "head"
```

```toml
# .repobundle.toml
[defaults]
prune = 2
```

a run keeps 2 bundles and bundles only the checked-out branch.

## `[defaults]`

| Key           | Default                              | Meaning                                                              |
| ------------- | ------------------------------------ | -------------------------------------------------------------------- |
| `output`      | `"bundles"`                          | Where bundles go. A path ending in `.bundle` is a single file.       |
| `name`        | `"{timestamp}-{hash}-{repo}.bundle"` | Filename template for a directory output.                            |
| `refs`        | `"all"`                              | Which refs to bundle. One of `"all"`, `"branches"`, or `"head"`.     |
| `prune`       | _(unset)_                            | After creating a bundle, keep only the newest N. Unset never prunes. |
| `max_size_mb` | _(unset)_                            | Warn when a bundle is larger than this. Unset never warns.           |
| `repo_name`   | _(unset)_                            | The name used for `{repo}`. Unset uses the repo directory's name.    |

### `output`

Where new bundles are written.

- A relative path resolves against the **repo root**, not the current directory. This is different from `-o`, which resolves against the current directory.
- A leading `~` expands to your home directory. Only `~` on its own expands, so `~alice/bundles` is treated as a relative path.
- A path ending in `.bundle` names a single file. Each new bundle replaces it, and `name` and `prune` are ignored, since there is no directory to name bundles in or prune.
- Anything else is a directory. It's created on the first run if it doesn't exist.

```toml
[defaults]
# One directory for every repo's bundles
output = "~/bundles"
```

**Sharing a directory between repos.** Bundles are matched to a repo by name, so two repos with the same directory name that share an output directory see each other's bundles. Pruning one can delete the other's. If you point the global `output` at a shared directory, set `repo_name` in the project file of any repo whose name isn't unique.

**Output inside the repo.** If the output is inside the repo and git doesn't ignore it, every run warns that git lists it as untracked. Add it to `.gitignore`, or to `.git/info/exclude` to keep the rule local.

```bash
echo "/bundles/" >> .git/info/exclude
```

### `name`

The filename template for bundles in a directory output. The default produces names like `20261004T153012Z-a1b2c3d-myrepo.bundle`.

| Placeholder   | Value                                                    | Example            |
| ------------- | -------------------------------------------------------- | ------------------ |
| `{timestamp}` | Creation time in UTC, `YYYYMMDDTHHMMSSZ`                 | `20261004T153012Z` |
| `{date}`      | Creation date in UTC, `YYYYMMDD`                         | `20261004`         |
| `{hash}`      | HEAD's short hash, always at least 7 characters          | `a1b2c3d`          |
| `{repo}`      | The repo name (see [`repo_name`](#repo_name))            | `myrepo`           |
| `{branch}`    | The checked-out branch, or `detached` on a detached HEAD | `feature-login`    |

`{branch}` replaces every character other than letters, digits, `.`, `_`, and `-` with `-`, so `feature/login` becomes `feature-login`.

Times are in UTC so names never depend on your timezone or daylight saving. The format has no colons, so it's a valid filename on every OS, and it sorts correctly as plain text.

#### Template Rules

repobundle reads its own filenames back to find a repo's bundles for the skip check and for pruning. A template is rejected unless its names can be read back without ambiguity, which means it must

- end in `.bundle`
- contain `{repo}`
- contain no `/` or `\`
- use each placeholder at most once
- use only the placeholders above, with every `{` and `}` matched

`{hash}` and `{branch}` also can't run straight into `{repo}`. The text between them must include a character the neighbor can't contain. In `{branch}-{repo}.bundle`, a branch named `x` in repo `my-app` and a branch named `x-my` in repo `app` would both render `x-my-app.bundle`. Use `@` instead.

| Template                        | Valid | Why                                    |
| ------------------------------- | ----- | -------------------------------------- |
| `{date}-{branch}@{repo}.bundle` | Yes   | `@` can't appear in a branch name here |
| `{hash}-{repo}.bundle`          | Yes   | `-` can't appear in a hash             |
| `{branch}-{repo}.bundle`        | No    | `-` can appear in a branch name        |
| `{repo}.{branch}.bundle`        | No    | `.` can appear in a branch name        |
| `{timestamp}-{hash}.bundle`     | No    | No `{repo}`                            |

An invalid template stops the run with the rule it broke. Passed with `--name`, it's rejected by the argument parser, which uses its own `error:` format.

```bash
repobundle --name '{branch}-{repo}.bundle'
```

```
error: invalid value '{branch}-{repo}.bundle' for '--name <TEMPLATE>': name template must separate `{branch}` from `{repo}` with a character that `{branch}` can't contain, such as `@`

For more information, try '--help'.
```

In a config file, the same rule appears after `[!] Config file <path> is malformed`.

#### Ordering and Repeated Names

"Newest" means most recently created, not most recent commit. Bundles are ordered by the `{timestamp}` in their names, then by file modification time. A template without `{timestamp}` is ordered by modification time alone, so copying or touching old bundles can change which ones a prune keeps.

A template that can render the same name twice replaces the earlier bundle with that name. `{repo}.bundle` keeps one bundle per repo, and `{date}-{repo}.bundle` keeps one per day. Keep `{timestamp}` in the template if you want every run to be kept.

### `refs`

Which refs go into the bundle. Every bundle also includes `HEAD`, so a clone from it always has a working tree.

| Value        | Contents                                                                  |
| ------------ | ------------------------------------------------------------------------- |
| `"all"`      | Every ref, including remote-tracking branches, tags, notes, and the stash |
| `"branches"` | Local branches and tags                                                   |
| `"head"`     | Only the checked-out branch. On a detached HEAD, only `HEAD`              |

With `"all"`, a new bundle is created whenever any ref moves, even one on a branch you aren't working on. Use `"head"` if you only care about the branch you're on.

### `prune`

After creating a bundle, keep only the newest N bundles of this repo and delete the rest. The new bundle always counts as one of the N.

```toml
[defaults]
prune = 3
```

- It has no effect when `output` names a single file.
- `--no-prune` turns it off for one run.
- A bare `--prune` or `--prune-only` on the command line uses this count instead of 1.
- Deleting asks for confirmation unless you pass `--force`. See [Pruning](./pruning.md).

Only files whose names match the current `name` template for this repo are ever considered. If you change the template, bundles with the old naming are left alone.

### `max_size_mb`

Warn when a new bundle is larger than this many megabytes. A megabyte here is 1,000,000 bytes, the smaller of the units upload limits use, so the warning comes early rather than late.

```toml
[defaults]
max_size_mb = 30
```

```
[!] The bundle is 42.3 MB, over the `max_size_mb` limit of 30 MB
```

The bundle is still created. The warning only tells you it may be too large to upload.

### `repo_name`

The name used for `{repo}` in place of the repo directory's name. It can't be empty or contain `/` or `\`.

```toml
[defaults]
repo_name = "myrepo"
```

Set it only in a project file. A global value gives every repo the same name, so their bundles mix together in a shared output directory.

## Errors

Config problems stop the run before anything is created or deleted.

- An unknown key or table is an error, so a typo like `max_size = 30` doesn't silently do nothing.
- `prune` and `max_size_mb` must be at least 1. Leave a key out to turn it off.
- A file that exists but can't be read or parsed is an error that names the file and the problem.

## See Also

- [Options](./options.md). The command-line flags that override these values.
- [Pruning](./pruning.md). Confirmation, `--dry-run`, and what gets deleted.
- [General Usage](./general_usage.md). Creating bundles and reading the output.
