#[macro_use]
extern crate clap;
extern crate serde;
extern crate toml;

#[macro_use]
mod macros;
mod args;
mod config;
mod deploy;
mod filesystem;
mod parse;

use std::env;
use std::process;

fn main() {
    // Parse arguments
    let matches = args::get_args();

    // Do the "implies" relation between verbose and dry_run
    let act = matches.occurrences_of("dry_run") == 0;
    let verbosity = matches.occurrences_of("verbose");
    // If dry run, then at least one verbosity level.
    let verbosity = if act {
        verbosity
    } else {
        std::cmp::max(1, verbosity)
    };

    // Change dir
    let dir = matches.value_of("directory").unwrap();
    verb!(verbosity, 1, "Changing directory to {}", dir);
    if env::set_current_dir(dir).is_err() {
        println!("Error: No such directory {}", dir);
        process::exit(1);
    }

    verb!(verbosity, 3, "{:?}", matches);

    // Execute subcommand
    if let Some(specific) = matches.subcommand_matches("deploy") {
        deploy::deploy(&matches, specific, verbosity, act);
    } else if let Some(specific) = matches.subcommand_matches("config") {
        config::config(&matches, specific, verbosity, act);
    } else {
        unreachable!();
    }
}
