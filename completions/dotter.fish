complete -c dotter -n "__fish_use_subcommand" -s g -l global-config -d 'Location of the global configuration' -r -F
complete -c dotter -n "__fish_use_subcommand" -s l -l local-config -d 'Location of the local configuration' -r -F
complete -c dotter -n "__fish_use_subcommand" -l cache-file -d 'Location of cache file' -r -F
complete -c dotter -n "__fish_use_subcommand" -l cache-directory -d 'Directory to cache into' -r -F
complete -c dotter -n "__fish_use_subcommand" -l pre-deploy -d 'Location of optional pre-deploy hook' -r -F
complete -c dotter -n "__fish_use_subcommand" -l post-deploy -d 'Location of optional post-deploy hook' -r -F
complete -c dotter -n "__fish_use_subcommand" -l pre-undeploy -d 'Location of optional pre-undeploy hook' -r -F
complete -c dotter -n "__fish_use_subcommand" -l post-undeploy -d 'Location of optional post-undeploy hook' -r -F
complete -c dotter -n "__fish_use_subcommand" -l diff-context-lines -d 'Amount of lines that are printed before and after a diff hunk' -r
complete -c dotter -n "__fish_use_subcommand" -s d -l dry-run -d 'Dry run - don\'t do anything, only print information. Implies -v at least once'
complete -c dotter -n "__fish_use_subcommand" -s v -l verbose -d 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter\'s run'
complete -c dotter -n "__fish_use_subcommand" -s q -l quiet -d 'Quiet - only print errors'
complete -c dotter -n "__fish_use_subcommand" -s f -l force -d 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
complete -c dotter -n "__fish_use_subcommand" -s y -l noconfirm -d 'Assume "yes" instead of prompting when removing empty directories'
complete -c dotter -n "__fish_use_subcommand" -s p -l patch -d 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
complete -c dotter -n "__fish_use_subcommand" -s h -l help -d 'Print help information'
complete -c dotter -n "__fish_use_subcommand" -s V -l version -d 'Print version information'
complete -c dotter -n "__fish_use_subcommand" -f -a "deploy" -d 'Deploy the files to their respective targets. This is the default subcommand'
complete -c dotter -n "__fish_use_subcommand" -f -a "undeploy" -d 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache'
complete -c dotter -n "__fish_use_subcommand" -f -a "init" -d 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package'
complete -c dotter -n "__fish_use_subcommand" -f -a "watch" -d 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`'
complete -c dotter -n "__fish_use_subcommand" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s g -l global-config -d 'Location of the global configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s l -l local-config -d 'Location of the local configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s d -l dry-run -d 'Dry run - don\'t do anything, only print information. Implies -v at least once'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s v -l verbose -d 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter\'s run'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s q -l quiet -d 'Quiet - only print errors'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s f -l force -d 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s y -l noconfirm -d 'Assume "yes" instead of prompting when removing empty directories'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s p -l patch -d 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
complete -c dotter -n "__fish_seen_subcommand_from deploy" -s h -l help -d 'Print help information'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s g -l global-config -d 'Location of the global configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s l -l local-config -d 'Location of the local configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s d -l dry-run -d 'Dry run - don\'t do anything, only print information. Implies -v at least once'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s v -l verbose -d 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter\'s run'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s q -l quiet -d 'Quiet - only print errors'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s f -l force -d 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s y -l noconfirm -d 'Assume "yes" instead of prompting when removing empty directories'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s p -l patch -d 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
complete -c dotter -n "__fish_seen_subcommand_from undeploy" -s h -l help -d 'Print help information'
complete -c dotter -n "__fish_seen_subcommand_from init" -s g -l global-config -d 'Location of the global configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from init" -s l -l local-config -d 'Location of the local configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from init" -s d -l dry-run -d 'Dry run - don\'t do anything, only print information. Implies -v at least once'
complete -c dotter -n "__fish_seen_subcommand_from init" -s v -l verbose -d 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter\'s run'
complete -c dotter -n "__fish_seen_subcommand_from init" -s q -l quiet -d 'Quiet - only print errors'
complete -c dotter -n "__fish_seen_subcommand_from init" -s f -l force -d 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
complete -c dotter -n "__fish_seen_subcommand_from init" -s y -l noconfirm -d 'Assume "yes" instead of prompting when removing empty directories'
complete -c dotter -n "__fish_seen_subcommand_from init" -s p -l patch -d 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
complete -c dotter -n "__fish_seen_subcommand_from init" -s h -l help -d 'Print help information'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s g -l global-config -d 'Location of the global configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from watch" -s l -l local-config -d 'Location of the local configuration' -r -F
complete -c dotter -n "__fish_seen_subcommand_from watch" -s d -l dry-run -d 'Dry run - don\'t do anything, only print information. Implies -v at least once'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s v -l verbose -d 'Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter\'s run'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s q -l quiet -d 'Quiet - only print errors'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s f -l force -d 'Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s y -l noconfirm -d 'Assume "yes" instead of prompting when removing empty directories'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s p -l patch -d 'Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch'
complete -c dotter -n "__fish_seen_subcommand_from watch" -s h -l help -d 'Print help information'
complete -c dotter -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from deploy; and not __fish_seen_subcommand_from undeploy; and not __fish_seen_subcommand_from init; and not __fish_seen_subcommand_from watch; and not __fish_seen_subcommand_from help" -f -a "deploy" -d 'Deploy the files to their respective targets. This is the default subcommand'
complete -c dotter -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from deploy; and not __fish_seen_subcommand_from undeploy; and not __fish_seen_subcommand_from init; and not __fish_seen_subcommand_from watch; and not __fish_seen_subcommand_from help" -f -a "undeploy" -d 'Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache'
complete -c dotter -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from deploy; and not __fish_seen_subcommand_from undeploy; and not __fish_seen_subcommand_from init; and not __fish_seen_subcommand_from watch; and not __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package'
complete -c dotter -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from deploy; and not __fish_seen_subcommand_from undeploy; and not __fish_seen_subcommand_from init; and not __fish_seen_subcommand_from watch; and not __fish_seen_subcommand_from help" -f -a "watch" -d 'Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`'
complete -c dotter -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from deploy; and not __fish_seen_subcommand_from undeploy; and not __fish_seen_subcommand_from init; and not __fish_seen_subcommand_from watch; and not __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
