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

Alternatively, Dotter is on [crates.io](crates.io/crates/dotter), so run `cargo install dotter` after installing rust to install it that way.

# Setup
- If you don't have a dotfile repository yet:
  - Figure out all the applications that use dotfiles that you want to keep track of, and write down where all those files are located.
  - Create a new git repo (I suggest placing it in `~/.dotfiles`)
  - Move those dotfiles to the repo. You can also change the names to make sense to you - for example a file that was at `~/.i3/config` can be renamed to simply `i3`.
  - Commit and push, you now have a dotfiles repo!
- In your dotfiles repo, create a folder `dotter_settings` and two files: `global.toml` and `local.toml`.
- Add `local.toml` to your `.gitignore` - that file contains the machine-specific configuration, so there's no point uploading it.
- When installing, I recommend downloading the binaries (windows and linux) into the root of your repository.\
  That way, wherever your dotfiles are, Dotter also is.
- On Linux, make sure `dotter` has execute permissions with `chmod +x dotter`, then you can run it with `./dotter`

# Configuration

Dotter operates under a concept of "packages". This comes from the idea that you don't always want all your dotfiles to be deployed - sometimes only part of the programs are installed.

In `global.toml`, packages are defined. Which packages are deployed is configured in `local.toml`.

Here's an example `global.toml`, that showcases several features:
```toml
# Helpers are user-defined functions that can be executed inside templates.
# This section is optional.
[helpers]
color_hex2rgb = "dotter_settings/helpers/color_hex2rgb.rhai"

# A package contains two sections - "files" and "variables".
# Both of those sections are optional - you can have only one if you want.

# The 'files' section is a mapping between the location of the file in the
# repository and its location in the filesystem (where the program expects it)
# On Windows, '~' is expanded to 'C:\Users\<USERNAME>\'
[zsh.files]
zprofile = "~/.zprofile"
zshrc = "~/.zshrc"

# The 'variables' section contains constants that the templated files
# can access. This section can contain all the types that toml supports,
# and is used by the Handlebars templating engine as the rendering context.
[zsh.variables]
prompt_color = "#00FF00"

# A package for all files relating to the i3 window manager
# I would only select it if I had i3 installed,
# so for example, I wouldn't select it on my VPS since it has no screen.

# In this package, I left comments to remind myself that it uses variables
# from the "graphics" package, and that I need to configure certain
# machine-specific variables if I want to use it.

# Note that variables from a selected package are available to all others.
[i3.files]  # requires "graphics"
Xinitrc = "~/.xinitrc"
i3 = "~/.i3/config"
polybar = "~/.config/polybar/config"
# Local variables: network_interface, screen_size, terminal

# A variables-only package, maybe it contains variables that are also
# used by my terminal so I want them to exist when I select either of
# the packages, without having to repeat them.
[graphics.variables]
font_size = 14
primary_color = "#CCCCCC"
background_color = "#333333"
```

As you can see, a `global.toml` contains a description of all the dotfiles that are *possible* to install.

But you don't always want all of them. Which packages are installed as well as machine-specific tweaks are configured in `local.toml`:
```toml
# An array of the names of all packages that are selected.
# Only the files and variables that belong to packages in this list are kept.

# When writing this line, I would consult the "requires" comments left in global.toml
# to see that I don't forget any required packages.
packages = ["i3", "graphics"]

# I need to define some machine-specific variables.
[i3.variables]
network_interface = "wlan0"
screen_size = "1920x1080"
terminal = "xfce4-terminal"

# File target locations can be overridden in local.toml
# This can be for example useful for applications which read from a diferent
#  location depending on the platform.
# Disabling files is possible by setting them to the special value `false`.
[i3.files]
Xinitrc = "~/.my_Xinitrc"
polybar = false

# Actually, I want the font size on this screen to be a bit bigger.
# Any variables defined in local.toml override variables in global.toml.
# Unlike files, it's impossible to delete variables.
[graphics.variables]
font_size = 18
```

For the initial configuration, you might want to have a single `always` package that contains just a `files` section, then select it in `local.toml`.
You can always break it up into smaller packages later!

For an example of a repository that uses dotter, check out [my dotfiles](github.com/SuperCuber/dotfiles). The folder of interest is `dotter_settings`.

## Templating
Dotter uses [Handlebars](https://handlebarsjs.com/guide/) as its rendering engine.
I recommend reading through that link to learn the full capabilities of the engine, but the most important feature is that it will substitute `{{variable_name}}` with the variable's value.

So, if your configuration had the variables `name = "Jack"` and `surname = "Black"`, rendering the file
```
Hello, {{name}} {{surname}}!
```
will result in

```
Hello, Jack Black!
```

This is useful for the same reason constants in code are - when you have repeating values many times in the same file, or have the same value repeated in many files, it's useful to have it declared once then templated into everywhere it's used. Then, if you change it in the declaration, it will automatically change everywhere its referenced.

Some examples could be: colors in your colorscheme can be automatically shared between all applications, font sizes can be kept consistent accross applications, etc.

Handlebars is more than simple find-and-replace though - it has many other features, like `if` blocks.\
For example, if `age = 8` then:
```
Jonny can{{#if (lt age 18)}}not{{/if}} drink alcohol
```
Will render as:
```
Jonny cannot drink alcohol
```

### Helpers
Handlebars supports custom helpers - for example, a helper that will convert a color from hex to rgb named `hex2rgb` will be used like so: `{{hex_to_rgb background_color}}`.\
The Rust implementation of handlebars supports custom helpers written in [rhai](https://github.com/jonathandturner/rhai) which is a scripting language.

To define one yourself, add it in the optional `[helpers]` section in `global.toml` in the form of `script_name = "script_file_location"`.\
The ones I currently use will be in the [helpers folder of my dotfiles](https://github.com/SuperCuber/dotfiles/tree/master/dotter_settings/helpers) - you can use them as examples.

Additionally, there's helpers implemented in rust.\
The ones that currently exist:
- `math` - used like `{{math font_size "*" 2}}`. Executed by [meval-rs](https://github.com/rekka/meval-rs#supported-expressions).

## Caching, and Templated vs Untemplated
TODO: This functionality will be reworked soon (see [issue #6](https://github.com/SuperCuber/dotter/issues/6)) so I won't document it yet.

# Usage
Now that you've configured all the global and local file sections, you can simply run `dotter` from within your repository.\
All the files will be deployed to their target locations.

Check out `dotter -h` for the commandline flags that Dotter supports.

Dotter uses the `env_logger` rust library for displaying errors and warnings. To configure logging level, use the `RUST_LOG` environment variable. The options are, in order of least verbose to most verbose: `error`, `warn`, `info`, `debug`, `trace`. The default is `error`.

# Contributing
Contributions to Dotter are welcome, whether in the form of a pull request or an issue (for bug repots, feature requests, or other helpful comments)
