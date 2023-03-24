
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'dotter' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'dotter'
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
        'dotter' {
            [CompletionResult]::new('-g', 'g', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('--global-config', 'global-config', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--local-config', 'local-config', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--cache-file', 'cache-file', [CompletionResultType]::ParameterName, 'Location of cache file')
            [CompletionResult]::new('--cache-directory', 'cache-directory', [CompletionResultType]::ParameterName, 'Directory to cache into')
            [CompletionResult]::new('--pre-deploy', 'pre-deploy', [CompletionResultType]::ParameterName, 'Location of optional pre-deploy hook')
            [CompletionResult]::new('--post-deploy', 'post-deploy', [CompletionResultType]::ParameterName, 'Location of optional post-deploy hook')
            [CompletionResult]::new('--pre-undeploy', 'pre-undeploy', [CompletionResultType]::ParameterName, 'Location of optional pre-undeploy hook')
            [CompletionResult]::new('--post-undeploy', 'post-undeploy', [CompletionResultType]::ParameterName, 'Location of optional post-undeploy hook')
            [CompletionResult]::new('--diff-context-lines', 'diff-context-lines', [CompletionResultType]::ParameterName, 'Amount of lines that are printed before and after a diff hunk')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('--dry-run', 'dry-run', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('--force', 'force', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('-y', 'y', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('--noconfirm', 'noconfirm', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('--patch', 'patch', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('deploy', 'deploy', [CompletionResultType]::ParameterValue, 'Deploy the files to their respective targets. This is the default subcommand')
            [CompletionResult]::new('undeploy', 'undeploy', [CompletionResultType]::ParameterValue, 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package')
            [CompletionResult]::new('watch', 'watch', [CompletionResultType]::ParameterValue, 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'dotter;deploy' {
            [CompletionResult]::new('-g', 'g', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('--global-config', 'global-config', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--local-config', 'local-config', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('--dry-run', 'dry-run', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('--force', 'force', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('-y', 'y', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('--noconfirm', 'noconfirm', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('--patch', 'patch', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            break
        }
        'dotter;undeploy' {
            [CompletionResult]::new('-g', 'g', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('--global-config', 'global-config', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--local-config', 'local-config', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('--dry-run', 'dry-run', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('--force', 'force', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('-y', 'y', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('--noconfirm', 'noconfirm', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('--patch', 'patch', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            break
        }
        'dotter;init' {
            [CompletionResult]::new('-g', 'g', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('--global-config', 'global-config', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--local-config', 'local-config', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('--dry-run', 'dry-run', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('--force', 'force', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('-y', 'y', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('--noconfirm', 'noconfirm', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('--patch', 'patch', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            break
        }
        'dotter;watch' {
            [CompletionResult]::new('-g', 'g', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('--global-config', 'global-config', [CompletionResultType]::ParameterName, 'Location of the global configuration')
            [CompletionResult]::new('-l', 'l', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('--local-config', 'local-config', [CompletionResultType]::ParameterName, 'Location of the local configuration')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('--dry-run', 'dry-run', [CompletionResultType]::ParameterName, 'Dry run - don''t do anything, only print information. Implies -v at least once')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Quiet - only print errors')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('--force', 'force', [CompletionResultType]::ParameterName, 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run')
            [CompletionResult]::new('-y', 'y', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('--noconfirm', 'noconfirm', [CompletionResultType]::ParameterName, 'Assume "yes" instead of prompting when removing empty directories')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('--patch', 'patch', [CompletionResultType]::ParameterName, 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            break
        }
        'dotter;help' {
            [CompletionResult]::new('deploy', 'deploy', [CompletionResultType]::ParameterValue, 'Deploy the files to their respective targets. This is the default subcommand')
            [CompletionResult]::new('undeploy', 'undeploy', [CompletionResultType]::ParameterValue, 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package')
            [CompletionResult]::new('watch', 'watch', [CompletionResultType]::ParameterValue, 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'dotter;help;deploy' {
            break
        }
        'dotter;help;undeploy' {
            break
        }
        'dotter;help;init' {
            break
        }
        'dotter;help;watch' {
            break
        }
        'dotter;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
