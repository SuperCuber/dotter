# What is Dotter?
Dotter is a dotfile manager and templater.

Dotfiles are *configuration* files that usually live in the home directory and start with a dot.
Often times, it is desirable to have a backup of all the configurations on your system, which is why a lot of users have their dotfiles saved in a git repository, then symlinking them to their target locations using `ln -s`.

However, there are several issues with that barebones approach:
- Hard to keep track of what comes from where once you have more than a handful of dotfiles
- Tedious to setup on a new machine - you need to manually create every single link
- No real way to handle differences between machines - say you want the battery meter on your bar to not appear on your desktop machine

Dotter aims to solve all those problems by providing a flexible configuration and automatic templating or symlinking to the target locations.

⚠️ **THIS PROJECT IS UNDER HEAVY DEVELOPMENT**. I use it regularly myself so it's reasonably tested, but expect bugs to happen.
If you find a bug, please open an issue describing how to reproduce it, and it will get fixed.\
I create Releases often so make sure you check whether the bug was fixed in the latest one!

# Installation
Download the binary for your platform from the latest release and then put it in your `$PATH` or in your dotfile repository (then you'd run it with `./dotter`).

Alternatively, Dotter is on [crates.io](https://crates.io/crates/dotter), so run `cargo install dotter` after installing rust to install it that way.

# Wiki
Check out [the wiki](https://github.com/SuperCuber/dotter/wiki) for more information.
Among other things, it explains how to setup and configure Dotter, as well as giving insight on how the templating and deployment works.

# Usage
Now that you've configured all the global and local file sections, you can simply run `dotter` from within your repository.\
All the files will be deployed to their target locations.

Check out `dotter -h` for the command-line flags that Dotter supports:

```
Dotter 0.7.2
A small dotfile manager.

USAGE:
    dotter [FLAGS] [OPTIONS]

FLAGS:
        --dry-run     Dry run - don't do anything, only print information. Implies RUST_LOG=info unless specificed
                      otherwise
        --force       Force - instead of skipping, overwrite target files if their content is unexpected. Overrides
                      --dry-run and implies RUST_LOG=warn unless specified otherwise
    -h, --help        Prints help information
        --undeploy    Un-deploy - delete all deployed files in their target locations. Note that this operates on all
                      files that are currently in cache
    -V, --version     Prints version information

OPTIONS:
        --cache-directory <cache-directory>    Directory to cache into [default: dotter_settings/cache]
        --cache-file <cache-file>              Location of cache file [default: dotter_settings/cache.toml]
    -d, --directory <directory>                Do all operations relative to this directory [default: .]
    -g, --global-config <global-config>        Location of the global configuration [default:
                                               dotter_settings/global.toml]
    -l, --local-config <local-config>          Location of the local configuration [default: dotter_settings/local.toml]
```

Dotter uses the `env_logger` rust library for displaying errors and warnings. To configure logging level, use the `RUST_LOG` environment variable. The options are, in order of least verbose to most verbose: `error`, `warn`, `info`, `debug`, `trace`. The default is `error`.

# Contributing
Contributions to Dotter are welcome, whether in the form of a pull request or an issue (for bug repots, feature requests, or other helpful comments)
