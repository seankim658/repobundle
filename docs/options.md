# Options

```
Create, verify, and prune git bundles of a repository

Usage: repobundle [OPTIONS] [PATH]
       repobundle <COMMAND>

Commands:
  completion  Print a shell completion script
  help        Print this message or the help of the given subcommand(s)

Arguments:
  [PATH]  Repository to bundle, or any directory inside it [default: .]

Options:
  -o, --output <PATH>     Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory
      --name <TEMPLATE>   Filename template used when the output is a directory
      --refs <REFS>       Choose which refs to bundle [possible values: all, branches, head]
      --prune[=<N>]       After creating, keep only the newest N bundles of this repo
      --prune-only[=<N>]  Keep only the newest N bundles of this repo without creating one
      --no-prune          Ignore the `prune` value from config files
      --dry-run           Report what would be created and deleted without doing either
      --force             Create even if an identical bundle exists
  -y, --yes               Delete bundles over the prune limit without asking
      --no-warnings       Suppress all warnings
      --verbose           Log every git command and its exit status
  -h, --help              Print help
  -V, --version           Print version
```

**Prune counts need `=`.** Write `--prune=3`, not `--prune 3`. Without the `=`, the `3` is read as the `PATH` to bundle. A bare `--prune` or `--prune-only` uses the `prune` count from your [config file](./config_file.md#prune), or 1 if none is set.

**A directory named `completion`.** `completion` is a subcommand, so `repobundle completion` prints completions instead of bundling a directory with that name. Write `repobundle ./completion` to bundle it.

**`-o` is relative to where you run.** A relative `-o` resolves against the current directory. The `output` key in a config file resolves against the repo root instead.

**File outputs.** With `-o` ending in `.bundle`, `--name`, `--prune`, and `--prune-only` are errors, and `name` and `prune` from config files are ignored. See [Writing a Single File](./general_usage.md#writing-a-single-file).

**`--force` and `--yes` are separate.** `--force` creates a new bundle even when nothing has changed. `--yes` deletes bundles over the prune limit without asking. `--force` can't be combined with `--prune-only`, which creates nothing. See [Scripts and Scheduled Runs](./pruning.md#scripts-and-scheduled-runs) for pruning without a terminal.

**`--dry-run` wins.** With `--dry-run`, nothing is written or deleted, even alongside `--force` or `--yes`.

**Defaults.** `-o`, `--name`, `--refs`, and the prune count can also be set in a [config file](./config_file.md), along with `max_size_mb` and `repo_name`, which have no flag. Command-line flags always win.

**Exit codes.** 0 on success, including when the bundle was already up to date or a prune was declined. 1 on any error. 2 for a mistake in the command line itself, such as an unknown flag or `--prune=0`.

## See Also

- [General Usage](./general_usage.md). What each run does and what it prints.
- [Pruning](./pruning.md). `--prune`, `--prune-only`, confirmation, and `--dry-run`.
- [Config File](./config_file.md). Setting defaults for these options.
