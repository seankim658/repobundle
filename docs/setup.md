# Setup

This guide will help you install repobundle, set it up to run from anywhere, and configure shell completions.

- [Requirements](#requirements)
- [Installation](#installation)
- [Running the Tool](#running-the-tool)
- [Setting up Shell Completions](#setting-up-shell-completions)
- [Optional Config](#optional-config)

---

## Requirements

- [git](https://git-scm.com/downloads) on your `PATH`. repobundle runs every operation through the `git` binary and stops with an error if it can't find it.
- [Rust](https://doc.rust-lang.org/book/ch01-01-installation.html) and Cargo (installed with Rust), to build from source.

## Installation

Clone the repository and build a release binary.

```bash
git clone git@github.com:seankim658/repobundle.git
cd repobundle/
cargo build --release
```

The binary is written to `target/release/repobundle`.

## Running the Tool

You can run the tool by passing the full path to the binary. For convenience, you have a few options to run it as plain `repobundle` instead.

1. Install With Cargo

From the repo directory, let Cargo build the binary and copy it into `~/.cargo/bin`, which the Rust installer already adds to your `PATH`.

```bash
cargo install --path .
```

Run the same command again after pulling changes to update it.

2. Create an Alias

Open your shell configuration file (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`, etc.) and add

```bash
alias repobundle='path/to/repobundle/target/release/repobundle'
```

3. Add to System PATH

Alternatively, copy the binary into a directory that's already on your `PATH`.

```bash
sudo cp target/release/repobundle /usr/local/bin/
```

After choosing an alias or a `PATH` change, apply it by reloading your shell configuration.

```bash
# For bash
source ~/.bashrc

# For zsh
source ~/.zshrc

# For fish
source ~/.config/fish/config.fish
```

Check that it works from any directory.

```bash
repobundle --version
```

## Setting up Shell Completions

repobundle can generate completion scripts for Bash, Zsh, Fish, PowerShell, and Elvish. The script is printed to stdout.

```bash
repobundle completion <shell> > repobundle-completion.<shell>
```

Replace `<shell>` with `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

You can place completion scripts where you prefer, but some standard locations and setup steps for each shell are below.

For Bash

```bash
mkdir -p ~/.local/share/bash-completion/completions
repobundle completion bash > ~/.local/share/bash-completion/completions/repobundle
```

This location is picked up automatically when the `bash-completion` package is installed. If you put the script somewhere else, source it from your `.bashrc`.

For Zsh

```bash
mkdir -p ~/.zsh/completion
repobundle completion zsh > ~/.zsh/completion/_repobundle
echo 'fpath=(~/.zsh/completion $fpath)' >> ~/.zshrc
echo 'autoload -U compinit; compinit' >> ~/.zshrc
```

The file must be named `_repobundle` for Zsh to find it.

For Fish

```fish
mkdir -p ~/.config/fish/completions
repobundle completion fish > ~/.config/fish/completions/repobundle.fish
```

Fish loads this directory automatically.

For PowerShell

```ps1
New-Item -ItemType Directory -Force $HOME\Documents\PowerShell\Completions
repobundle completion powershell > $HOME\Documents\PowerShell\Completions\repobundle.ps1
Add-Content $PROFILE '. $HOME\Documents\PowerShell\Completions\repobundle.ps1'
```

After installing the completions, open a new shell or re-source your config file.

The completion scripts describe the flags of the version that generated them. Regenerate them after updating repobundle.

## Optional Config

repobundle works with no configuration. To change its defaults, such as where bundles go or how many to keep, copy the sample config to your home directory and edit it.

```bash
cp repobundle.example.toml ~/.repobundle.toml
```

See [Config File](./config_file.md) for every key, and [General Usage](./general_usage.md) to make your first bundle.
