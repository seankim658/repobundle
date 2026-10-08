
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'repobundle' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'repobundle'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'repobundle' {
            [CompletionResult]::new('-o', '-o', [CompletionResultType]::ParameterName, 'Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Write the bundle here. A path ending in `.bundle` is a file, anything else is a directory')
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Filename template used when the output is a directory')
            [CompletionResult]::new('--refs', '--refs', [CompletionResultType]::ParameterName, 'Choose which refs to bundle')
            [CompletionResult]::new('--prune', '--prune', [CompletionResultType]::ParameterName, 'After creating, keep only the newest N bundles of this repo')
            [CompletionResult]::new('--prune-only', '--prune-only', [CompletionResultType]::ParameterName, 'Keep only the newest N bundles of this repo without creating one')
            [CompletionResult]::new('--no-prune', '--no-prune', [CompletionResultType]::ParameterName, 'Ignore the `prune` value from config files')
            [CompletionResult]::new('--list', '--list', [CompletionResultType]::ParameterName, 'List this repo''s bundles, newest first, without creating one')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Report what would be created and deleted without doing either')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Create even if an identical bundle exists')
            [CompletionResult]::new('--include-wip', '--include-wip', [CompletionResultType]::ParameterName, 'Bundle uncommitted changes to tracked files as refs/wip/repobundle')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Delete bundles over the prune limit without asking')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Delete bundles over the prune limit without asking')
            [CompletionResult]::new('--no-warnings', '--no-warnings', [CompletionResultType]::ParameterName, 'Suppress all warnings')
            [CompletionResult]::new('--no-spinner', '--no-spinner', [CompletionResultType]::ParameterName, 'Don''t show a spinner while the bundle is written')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Print the result as one JSON object on stdout')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Log every git command and its exit status')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('completion', 'completion', [CompletionResultType]::ParameterValue, 'Print a shell completion script')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'repobundle;completion' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'repobundle;help' {
            [CompletionResult]::new('completion', 'completion', [CompletionResultType]::ParameterValue, 'Print a shell completion script')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'repobundle;help;completion' {
            break
        }
        'repobundle;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
