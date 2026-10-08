# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_repobundle_global_optspecs
    string join \n o/output= name= refs= prune= prune-only= no-prune list dry-run force include-wip y/yes no-warnings no-spinner json verbose h/help V/version
end

function __fish_repobundle_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_repobundle_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_repobundle_using_subcommand
    set -l cmd (__fish_repobundle_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c repobundle -n "__fish_repobundle_needs_command" -s o -l output -d 'Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory' -r -F
complete -c repobundle -n "__fish_repobundle_needs_command" -l name -d 'Filename template used when the output is a directory' -r
complete -c repobundle -n "__fish_repobundle_needs_command" -l refs -d 'Choose which refs to bundle' -r -f -a "all\t''
branches\t''
head\t''"
complete -c repobundle -n "__fish_repobundle_needs_command" -l prune -d 'After creating, keep only the newest N bundles of this repo' -r
complete -c repobundle -n "__fish_repobundle_needs_command" -l prune-only -d 'Keep only the newest N bundles of this repo without creating one' -r
complete -c repobundle -n "__fish_repobundle_needs_command" -l no-prune -d 'Ignore the `prune` value from config files'
complete -c repobundle -n "__fish_repobundle_needs_command" -l list -d 'List this repo\'s bundles, newest first, without creating one'
complete -c repobundle -n "__fish_repobundle_needs_command" -l dry-run -d 'Report what would be created and deleted without doing either'
complete -c repobundle -n "__fish_repobundle_needs_command" -l force -d 'Create even if an identical bundle exists'
complete -c repobundle -n "__fish_repobundle_needs_command" -l include-wip -d 'Bundle uncommitted changes to tracked files as refs/wip/repobundle'
complete -c repobundle -n "__fish_repobundle_needs_command" -s y -l yes -d 'Delete bundles over the prune limit without asking'
complete -c repobundle -n "__fish_repobundle_needs_command" -l no-warnings -d 'Suppress all warnings'
complete -c repobundle -n "__fish_repobundle_needs_command" -l no-spinner -d 'Don\'t show a spinner while the bundle is written'
complete -c repobundle -n "__fish_repobundle_needs_command" -l json -d 'Print the result as one JSON object on stdout'
complete -c repobundle -n "__fish_repobundle_needs_command" -l verbose -d 'Log every git command and its exit status'
complete -c repobundle -n "__fish_repobundle_needs_command" -s h -l help -d 'Print help'
complete -c repobundle -n "__fish_repobundle_needs_command" -s V -l version -d 'Print version'
complete -c repobundle -n "__fish_repobundle_needs_command" -a "completion" -d 'Print a shell completion script'
complete -c repobundle -n "__fish_repobundle_needs_command" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c repobundle -n "__fish_repobundle_using_subcommand completion" -s h -l help -d 'Print help'
complete -c repobundle -n "__fish_repobundle_using_subcommand help; and not __fish_seen_subcommand_from completion help" -f -a "completion" -d 'Print a shell completion script'
complete -c repobundle -n "__fish_repobundle_using_subcommand help; and not __fish_seen_subcommand_from completion help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
