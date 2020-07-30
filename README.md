## Description
This is a small dotfile manager and templater.
It manages copying and updating (dot)files from a single directory into their respective locations.

## Installation
Download the binary for your platform from the latest release and then put it in your `$PATH`.

Alternatively, dotter is on crates.io, so run `cargo install dotter` after installing rust.

## Help
Find out help with `dotter -h`.

Variables are templated by putting `{{ VAR_NAME }}` in any file, and it will be
replaced with the configured value.

Feel free to fork/pull request/open any issues :)
