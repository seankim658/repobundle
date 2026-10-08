
use builtin;
use str;

set edit:completion:arg-completer[repobundle] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'repobundle'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'repobundle'= {
            cand -o 'Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory'
            cand --output 'Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory'
            cand --name 'Filename template used when the output is a directory'
            cand --refs 'Choose which refs to bundle'
            cand --prune 'After creating, keep only the newest N bundles of this repo'
            cand --prune-only 'Keep only the newest N bundles of this repo without creating one'
            cand --no-prune 'Ignore the `prune` value from config files'
            cand --list 'List this repo''s bundles, newest first, without creating one'
            cand --dry-run 'Report what would be created and deleted without doing either'
            cand --force 'Create even if an identical bundle exists'
            cand --include-wip 'Bundle uncommitted changes to tracked files as refs/wip/repobundle'
            cand -y 'Delete bundles over the prune limit without asking'
            cand --yes 'Delete bundles over the prune limit without asking'
            cand --no-warnings 'Suppress all warnings'
            cand --no-spinner 'Don''t show a spinner while the bundle is written'
            cand --json 'Print the result as one JSON object on stdout'
            cand --verbose 'Log every git command and its exit status'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand completion 'Print a shell completion script'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'repobundle;completion'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'repobundle;help'= {
            cand completion 'Print a shell completion script'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'repobundle;help;completion'= {
        }
        &'repobundle;help;help'= {
        }
    ]
    $completions[$command]
}
