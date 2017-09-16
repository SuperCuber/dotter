## Description
This is a small dotfile manager and templater.
It manages copying and updating (dot)files from a single directory into their respective locations.

## Installation
Dotter is now on crates.io, so just do `cargo install dotter` after installing rust.

## Help
Find out help with `dotter -h`.
Examples are in the form of [bintest](https://www.github.com/SuperCuber/bintest)s.

Variables are templated by putting `{{ VAR_NAME }}` in any file, and it will be
replaced with the configured value.

Feel free to fork/pull request/open any issues :)
