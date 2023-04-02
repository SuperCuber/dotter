
use builtin;
use str;

set edit:completion:arg-completer[dotter] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'dotter'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'dotter'= {
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand --cache-file 'Location of cache file'
            cand --cache-directory 'Directory to cache into'
            cand --pre-deploy 'Location of optional pre-deploy hook'
            cand --post-deploy 'Location of optional post-deploy hook'
            cand --pre-undeploy 'Location of optional pre-undeploy hook'
            cand --post-undeploy 'Location of optional post-undeploy hook'
            cand --diff-context-lines 'Amount of lines that are printed before and after a diff hunk'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
            cand -V 'Print version information'
            cand --version 'Print version information'
            cand deploy 'Deploy the files to their respective targets. This is the default subcommand'
            cand undeploy 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache'
            cand init 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package'
            cand watch 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`'
            cand gen-completions 'Generate shell completions'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'dotter;deploy'= {
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
        }
        &'dotter;undeploy'= {
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
        }
        &'dotter;init'= {
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
        }
        &'dotter;watch'= {
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
        }
        &'dotter;gen-completions'= {
            cand -s 'Set the shell for generating completions [values: bash, elvish, fish, powershell, zsh, nushell]'
            cand --shell 'Set the shell for generating completions [values: bash, elvish, fish, powershell, zsh, nushell]'
            cand -g 'Location of the global configuration'
            cand --global-config 'Location of the global configuration'
            cand -l 'Location of the local configuration'
            cand --local-config 'Location of the local configuration'
            cand -d 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand --dry-run 'Dry run - don''t do anything, only print information. Implies -v at least once'
            cand -v 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand --verbose 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter''s run'
            cand -q 'Quiet - only print errors'
            cand --quiet 'Quiet - only print errors'
            cand -f 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand --force 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
            cand -y 'Assume "yes" instead of prompting when removing empty directories'
            cand --noconfirm 'Assume "yes" instead of prompting when removing empty directories'
            cand -p 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand --patch 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
            cand -h 'Print help information'
            cand --help 'Print help information'
        }
        &'dotter;help'= {
            cand deploy 'Deploy the files to their respective targets. This is the default subcommand'
            cand undeploy 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache'
            cand init 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package'
            cand watch 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`'
            cand gen-completions 'Generate shell completions'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'dotter;help;deploy'= {
        }
        &'dotter;help;undeploy'= {
        }
        &'dotter;help;init'= {
        }
        &'dotter;help;watch'= {
        }
        &'dotter;help;gen-completions'= {
        }
        &'dotter;help;help'= {
        }
    ]
    $completions[$command]
}
