# Pruning

Every run that finds new commits adds a bundle, so a busy repo's `bundles/` directory grows quickly. Pruning keeps only the newest few bundles of a repo and deletes the rest.

Pruning only works with a directory output. A [single-file output](./general_usage.md#writing-a-single-file) only ever holds one bundle, so there's nothing to prune.

- [Pruning After Creating](#pruning-after-creating)
- [Pruning Without Creating](#pruning-without-creating)
- [Choosing the Count](#choosing-the-count)
- [What Gets Deleted](#what-gets-deleted)
  - [Which Bundles Are Newest](#which-bundles-are-newest)
  - [Which Files Count](#which-files-count)
- [Confirming Deletion](#confirming-deletion)
  - [Previewing With `--dry-run`](#previewing-with---dry-run)
  - [Scripts and Scheduled Runs](#scripts-and-scheduled-runs)
- [See Also](#see-also)

---

## Pruning After Creating

`--prune=N` creates a bundle as usual, then keeps the newest `N` bundles of this repo and deletes the rest. The bundle from this run always counts as one of the `N`.

```bash
repobundle --prune=2
```

```
[✓] Created bundles/20261007T150211Z-c71d9e4-myrepo.bundle (149.0 kB)
  - bundles/20261006T091544Z-1b0e7a2-myrepo.bundle
  - bundles/20261005T174802Z-9d3f210-myrepo.bundle
Delete 2 bundles? [y/N] y
[✓] Deleted bundles/20261006T091544Z-1b0e7a2-myrepo.bundle
[✓] Deleted bundles/20261005T174802Z-9d3f210-myrepo.bundle
```

Here the directory held three older bundles. The new one and the newest older one (`20261007T133512Z-3f9a2c1`, not listed) are kept, and the other two are deleted.

If nothing changed since your last bundle, the run reuses it instead of creating one (see [When Nothing Has Changed](./general_usage.md#when-nothing-has-changed)). The reused bundle is the one protected from pruning.

Pruning happens last, after any warnings. If creating the bundle fails, nothing is pruned.

## Pruning Without Creating

`--prune-only=N` skips creating a bundle and only prunes.

```bash
repobundle --prune-only=1
```

It doesn't check the repo's state, so it prints no warnings and works even in a repo with no commits. Nothing is protected here, so `--prune-only=1` keeps exactly the single newest bundle.

## Choosing the Count

The count comes from the flag, the `prune` key in a [config file](./config_file.md#prune), or both.

| You pass         | Config `prune` | Result                      |
| ---------------- | -------------- | --------------------------- |
| nothing          | unset          | No pruning                  |
| nothing          | `3`            | Prune to 3 after every run  |
| `--no-prune`     | `3`            | No pruning this run         |
| `--prune`        | unset          | Prune to 1                  |
| `--prune`        | `3`            | Prune to 3                  |
| `--prune=2`      | any            | Prune to 2                  |
| `--prune-only`   | unset          | Prune to 1 without creating |
| `--prune-only`   | `3`            | Prune to 3 without creating |
| `--prune-only=2` | any            | Prune to 2 without creating |

A bare `--prune` uses your configured count, so with `prune = 3` in a config file, `--prune` and `--prune=3` do the same thing.

The count must be at least 1. `--prune`, `--prune-only`, and `--no-prune` can't be combined.

**Write the count with `=`.** `--prune 2` doesn't set the count. The `2` is read as the path of the repo to bundle, so the run fails or bundles the wrong directory. Always write `--prune=2`.

## What Gets Deleted

### Which Bundles Are Newest

"Newest" means most recently created, not most recent commit. Bundles are ordered by

1. the `{timestamp}` in the filename
2. the file's modification time, which breaks ties between bundles made in the same second
3. the filename, so the order is the same every time

If your [name template](./config_file.md#name) has no `{timestamp}`, modification time is the only real signal. Copying, restoring, or touching an old bundle makes it look new, and a prune could then keep it over a genuinely newer one.

### Which Files Count

Only files whose names match this repo's name template are ever considered. Everything else in the directory is left alone, including

- bundles of other repos
- files that don't fit the template, such as `notes.txt` or a bundle you renamed by hand
- directories, symlinks, and repobundle's own temporary `.tmp` files

That makes it safe to point several repos at one shared output directory. The exception is two repos with the **same name**. Their bundles match each other's template, so pruning one can delete the other's. Give one of them a distinct `repo_name` in its project config file.

If you change the name template, bundles made under the old template stop matching. They're no longer pruned, and they're no longer used for the up-to-date check either. Delete or rename them by hand.

## Confirming Deletion

Deleting asks first. repobundle lists the bundles it would delete on stderr, then waits for an answer.

```
  - bundles/20261006T091544Z-1b0e7a2-myrepo.bundle
  - bundles/20261005T174802Z-9d3f210-myrepo.bundle
Delete 2 bundles? [y/N]
```

Only `y` or `yes`, in any case, deletes. Anything else, including just pressing Enter, keeps everything.

```
[i] Nothing deleted
```

Declining isn't an error. The run still exits successfully, and a bundle it created is kept.

When there's nothing over the limit, there's nothing to ask. `--prune-only` reports it, and a run that creates a bundle prints only its result line.

```
[i] Nothing to prune
```

Pass `--force` to delete without asking.

### Previewing With `--dry-run`

`--dry-run` lists what would be deleted and deletes nothing, even with `--force`.

```bash
repobundle --prune=2 --dry-run
```

```
[i] Would create bundles/20261007T150211Z-c71d9e4-myrepo.bundle
[i] Would delete bundles/20261006T091544Z-1b0e7a2-myrepo.bundle
[i] Would delete bundles/20261005T174802Z-9d3f210-myrepo.bundle
```

The bundle the run would create counts toward the limit even though it isn't written, so the preview matches what a real run would delete.

### Scripts and Scheduled Runs

Without a terminal to answer the prompt, such as in a script, a cron job, or with input piped in, repobundle refuses to delete rather than guessing.

```
[!] Pruning would delete 2 bundles, but there is no terminal to confirm; pass --force to delete without asking
```

Nothing is deleted and the run exits with an error. If there's nothing to delete, there's nothing to confirm, and the run succeeds.

**With `prune` in a config file, this applies to every run.** The bundle is created first, then the prune is refused, so an unattended run makes its bundle but still exits with an error. In scripts, pass `--force` to prune without asking, or `--no-prune` to skip pruning.

```bash
# Bundle and prune with no prompt
repobundle --force

# Bundle only, ignoring the configured prune
repobundle --no-prune
```

Note that `--force` also skips the [up-to-date check](./general_usage.md#when-nothing-has-changed), so `repobundle --force` makes a new bundle every time. To keep the check and still prune without asking, run the two steps separately.

```bash
repobundle --no-prune && repobundle --prune-only --force
```

## See Also

- [Config File](./config_file.md). Setting a default `prune` count and the name template pruning relies on.
- [General Usage](./general_usage.md). Creating bundles and the up-to-date check.
- [Options](./options.md). Every flag in one place.
