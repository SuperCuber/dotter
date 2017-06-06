# Dotter

**Important note:** Dotter has been recently ported to Rust.
Configuration is now different. Check out examples.

## Description

This is a small dotfile manager and templater.
It manages copying and updating (dot)files from a single directory into their respective locations.

## Building
To build, run `cargo build --release`. The binary will be in `target/release/dotter`.
You can copy or symlink it to your desired location.

## Help
Find out help with `path/to/dotter -h`.
Examples are in the form of [bintest](https://www.github.com/SuperCuber/bintest)s.

Variables are templated by putting `{{ VAR_NAME }}` in any file, and it will be
replaced with the configured value.

Feel free to fork/pull request/open any issues :)
