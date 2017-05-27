#[macro_use] extern crate clap;
extern crate toml;
extern crate serde;
#[macro_use] extern crate serde_derive;

mod args;
#[macro_use] mod macros;
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
    if matches.subcommand_matches("deploy").is_some() {
        deploy(&matches, verbosity, act);
    } else if matches.subcommand_matches("config").is_some() {
        config(&matches, verbosity, act);
    } else {
        unreachable!();
    }
}

fn deploy(matches: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Deploy args: {:?}", matches);

    // Load configuration
    let configuration: parse::Config = parse::load_file(
            matches.value_of("config")
            .unwrap()).unwrap();
    verb!(verbosity, 2, "Configuration: {}", configuration);

    // Load secrets
    let mut secrets: toml::value::Table = parse::load_file(
            matches.value_of("secrets")
            .unwrap()).unwrap();
    verb!(verbosity, 2, "Secrets: {:?}", secrets);

    // Get files
    let files = match configuration.files {
        Some(files) => { files }
        None => {
            println!("Warning: No files in configuration.");
            toml::value::Table::new()
        }
    };

    verb!(verbosity, 2, "Files: {:?}", files);

    // Get variables and update with secrets
    let mut variables = match configuration.variables {
        Some(variables) => { variables }
        None => { toml::value::Table::new() }
    };
    variables.append(&mut secrets); // Secrets is now empty

    verb!(verbosity, 2, "Variables: {:?}", variables);
}

fn config(matches: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Config args: {:?}", matches);
}
