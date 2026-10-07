# repobundle

Command line tool for packing a git repository's full history into a single [git bundle](https://git-scm.com/docs/git-bundle) file, ready to upload to an LLM chat or hand to anyone who needs the whole repo in one file.

A bundle is a normal git file format. Anyone with git can clone from it like a remote, so the history, branches, and tags all come along. repobundle handles the parts around `git bundle` that are easy to get wrong. It names each bundle so you can tell them apart, verifies every bundle before keeping it, skips the work when nothing has changed, prunes old bundles, and warns you when a bundle won't contain what you expect.

- [Installation](#installation)
  - [Building From Source](#building-from-source)
- [Usage](#usage)
  - [Quick Start](#quick-start)
  - [Arguments](#arguments)
- [Configuration](#configuration)
- [Usage Guides](./docs/README.md)

---

## Installation

To use repobundle you'll need [git](https://git-scm.com/downloads) on your `PATH`, since every operation runs through the `git` binary. For more detailed steps, including setting up tab completions, see the [setup](./docs/setup.md) guide.

### Building From Source

To build from source you will need [Rust](https://doc.rust-lang.org/book/ch01-01-installation.html) and Cargo (installed with Rust).

First clone the repository.

```bash
git clone git@github.com:seankim658/repobundle.git
```

And then compile a release binary.

```bash
cd repobundle/
cargo build --release
```

The binary is written to `target/release/repobundle`.

## Usage

More detailed usage guides can be found [here](./docs/README.md).

### Quick Start

Run it with no arguments from anywhere inside a repository.

```bash
repobundle
```

```
Created /home/me/code/myrepo/bundles/20261007T133512Z-3f9a2c1-myrepo.bundle (148.2 kB)
warning: bundles is inside the repo but not ignored, so git lists it as untracked; add it to .gitignore or .git/info/exclude
```

The bundle lands in a `bundles/` directory at the repo root. The warning goes away once you ignore that directory.

```bash
echo "/bundles/" >> .git/info/exclude
```

Running it again before you commit anything reuses the bundle you already have.

```
Up to date: /home/me/code/myrepo/bundles/20261007T133512Z-3f9a2c1-myrepo.bundle
```

To get the repo back out of a bundle, clone it like any other remote.

```bash
git clone 20261007T133512Z-3f9a2c1-myrepo.bundle myrepo
```

### Arguments

More extensive documentation on the options can be found [here](./docs/options.md). The repobundle command line tool has the following arguments.

```txt
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
      --force             Create even if an identical bundle exists, and delete without asking
      --no-warnings       Suppress all warnings
      --verbose           Log every git command and its exit status
  -h, --help              Print help
  -V, --version           Print version
```

**Note.** `--prune` and `--prune-only` only take a count after `=`, as in `--prune=3`. Written as `--prune 3`, the `3` is read as the `PATH` to bundle.

## Configuration

repobundle reads optional defaults from `~/.repobundle.toml` and from a `.repobundle.toml` in the repo, so the flags you always pass can live in a file instead. A commented sample is in [`repobundle.example.toml`](./repobundle.example.toml), and every key is explained in the [config file](./docs/config_file.md) guide.

```toml
[defaults]
prune = 3
max_size_mb = 30
```
