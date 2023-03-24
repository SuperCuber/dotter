module completions {

  # A dotfile manager and templater written in rust
  export extern dotter [
    --global-config(-g): string # Location of the global configuration
    --local-config(-l): string # Location of the local configuration
    --cache-file: string      # Location of cache file
    --cache-directory: string # Directory to cache into
    --pre-deploy: string      # Location of optional pre-deploy hook
    --post-deploy: string     # Location of optional post-deploy hook
    --pre-undeploy: string    # Location of optional pre-undeploy hook
    --post-undeploy: string   # Location of optional post-undeploy hook
    --dry-run(-d)             # Dry run - don't do anything, only print information. Implies -v at least once
    --verbose(-v)             # Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
    --quiet(-q)               # Quiet - only print errors
    --force(-f)               # Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
    --noconfirm(-y)           # Assume "yes" instead of prompting when removing empty directories
    --patch(-p)               # Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
    --diff-context-lines: string # Amount of lines that are printed before and after a diff hunk
    --help(-h)                # Print help information
    --version(-V)             # Print version information
  ]

  # Deploy the files to their respective targets. This is the default subcommand
  export extern "dotter deploy" [
    --global-config(-g): string # Location of the global configuration
    --local-config(-l): string # Location of the local configuration
    --dry-run(-d)             # Dry run - don't do anything, only print information. Implies -v at least once
    --verbose(-v)             # Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
    --quiet(-q)               # Quiet - only print errors
    --force(-f)               # Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
    --noconfirm(-y)           # Assume "yes" instead of prompting when removing empty directories
    --patch(-p)               # Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
    --help(-h)                # Print help information
  ]

  # Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache
  export extern "dotter undeploy" [
    --global-config(-g): string # Location of the global configuration
    --local-config(-l): string # Location of the local configuration
    --dry-run(-d)             # Dry run - don't do anything, only print information. Implies -v at least once
    --verbose(-v)             # Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
    --quiet(-q)               # Quiet - only print errors
    --force(-f)               # Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
    --noconfirm(-y)           # Assume "yes" instead of prompting when removing empty directories
    --patch(-p)               # Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
    --help(-h)                # Print help information
  ]

  # Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package
  export extern "dotter init" [
    --global-config(-g): string # Location of the global configuration
    --local-config(-l): string # Location of the local configuration
    --dry-run(-d)             # Dry run - don't do anything, only print information. Implies -v at least once
    --verbose(-v)             # Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
    --quiet(-q)               # Quiet - only print errors
    --force(-f)               # Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
    --noconfirm(-y)           # Assume "yes" instead of prompting when removing empty directories
    --patch(-p)               # Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
    --help(-h)                # Print help information
  ]

  # Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`
  export extern "dotter watch" [
    --global-config(-g): string # Location of the global configuration
    --local-config(-l): string # Location of the local configuration
    --dry-run(-d)             # Dry run - don't do anything, only print information. Implies -v at least once
    --verbose(-v)             # Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
    --quiet(-q)               # Quiet - only print errors
    --force(-f)               # Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
    --noconfirm(-y)           # Assume "yes" instead of prompting when removing empty directories
    --patch(-p)               # Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
    --help(-h)                # Print help information
  ]

  # Print this message or the help of the given subcommand(s)
  export extern "dotter help" [
  ]

  # Deploy the files to their respective targets. This is the default subcommand
  export extern "dotter help deploy" [
  ]

  # Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache
  export extern "dotter help undeploy" [
  ]

  # Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package
  export extern "dotter help init" [
  ]

  # Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`
  export extern "dotter help watch" [
  ]

  # Print this message or the help of the given subcommand(s)
  export extern "dotter help help" [
  ]

}

use completions *
