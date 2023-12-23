# What is Dotter?
Dotter is a dotfile manager and templater.

Dotfiles are *configuration* files that usually live in the home directory and start with a dot.
Often times, it is desirable to have a backup of all the configurations on your system, which is why a lot of users have their dotfiles saved in a git repository, then symlinking them to their target locations using `ln -s`.

However, there are several issues with that barebones approach:
- Hard to keep track of what comes from where once you have more than a handful of dotfiles
- Tedious to setup on a new machine - you need to manually create every single link
- No real way to handle differences between machines - say you want the battery meter on your bar to not appear on your desktop machine

Dotter aims to solve all those problems by providing a flexible configuration and automatic templating or symlinking to the target locations.

# Installation
## Mac (Homebrew)
Dotter is available on homebrew using `brew install dotter`

## Arch Linux
The following AUR packages are available:
- [dotter-rs-bin](https://aur.archlinux.org/packages/dotter-rs-bin) for a precompiled version of the latest release
- [dotter-rs](https://aur.archlinux.org/packages/dotter-rs) for the latest release's source that is built on your machine
- [dotter-rs-git](https://aur.archlinux.org/packages/dotter-rs-git) for the latest commit on master that is built on your machine

All of those are maintained by [orhun](https://github.com/orhun/) - huge thanks to him!

## Windows
Dotter is available on [Scoop](https://scoop.sh). Run `scoop install dotter` to install the latest release.

## Others

Download the binary for your platform from the latest release and then put it in your `$PATH` or in your dotfile repository (then you'd run it with `./dotter`).
Alternatively, Dotter is on [crates.io](https://crates.io/crates/dotter), so if you have Rustup installed, run `cargo install dotter`.

# Wiki
Check out [the wiki](https://github.com/SuperCuber/dotter/wiki) for more information.
Among other things, it explains how to setup and configure Dotter, as well as giving insight on how the templating and deployment works.

# Usage
Now that you've configured all the global and local file sections, you can simply run `dotter` from within your repository.\
All the files will be deployed to their target locations.

Check out `dotter -h` for the command-line flags that Dotter supports:

```
A dotfile manager and templater written in rust

Usage: dotter [OPTIONS] [COMMAND]

Commands:
  deploy           Deploy the files to their respective targets. This is the default subcommand
  undeploy         Delete all deployed files from their target locations. Note that this operates on all files that are currently in cache
  init             Initialize global.toml with a single package containing all the files in the current directory pointing to a dummy value and a local.toml that selects that package
  watch            Run continuously, watching the repository for changes and deploying as soon as they happen. Can be ran with `--dry-run`
  gen-completions  Generate shell completions
  help             Print this message or the help of the given subcommand(s)

Options:
  -g, --global-config <GLOBAL_CONFIG>
          Location of the global configuration [default: .dotter/global.toml]
  -l, --local-config <LOCAL_CONFIG>
          Location of the local configuration [default: .dotter/local.toml]
      --cache-file <CACHE_FILE>
          Location of cache file [default: .dotter/cache.toml]
      --cache-directory <CACHE_DIRECTORY>
          Directory to cache into [default: .dotter/cache]
      --pre-deploy <PRE_DEPLOY>
          Location of optional pre-deploy hook [default: .dotter/pre_deploy.sh]
      --post-deploy <POST_DEPLOY>
          Location of optional post-deploy hook [default: .dotter/post_deploy.sh]
      --pre-undeploy <PRE_UNDEPLOY>
          Location of optional pre-undeploy hook [default: .dotter/pre_undeploy.sh]
      --post-undeploy <POST_UNDEPLOY>
          Location of optional post-undeploy hook [default: .dotter/post_undeploy.sh]
  -d, --dry-run
          Dry run - don't do anything, only print information. Implies -v at least once
  -v, --verbose...
          Verbosity level - specify up to 3 times to get more detailed output. Specifying at least once prints the differences between what was before and after Dotter's run
  -q, --quiet
          Quiet - only print errors
  -f, --force
          Force - instead of skipping, overwrite target files if their content is unexpected. Overrides --dry-run
  -y, --noconfirm
          Assume "yes" instead of prompting when removing empty directories
  -p, --patch
          Take standard input as an additional files/variables patch, added after evaluating `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch
      --diff-context-lines <DIFF_CONTEXT_LINES>
          Amount of lines that are printed before and after a diff hunk [default: 3]
  -h, --help
          Print help
  -V, --version
          Print version
```

# Contributing
Contributions to Dotter are welcome, whether in the form of a pull request or an issue (for bug repots, feature requests, or other helpful comments)

# Support
Like what I do and want to encourage me to continue?\
You can donate a small amount via [Paypal](https://www.paypal.com/cgi-bin/webscr?cmd=_s-xclick&hosted_button_id=329HKDXK9UB84).\
Donations are not expected but greatly appreciated.
