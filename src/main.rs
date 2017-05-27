#[macro_use] extern crate clap;
extern crate yaml_rust;

mod parse;
#[macro_use] mod macros;

use std::env;
use std::fs::File;
use std::io::Read;
use std::process;

use yaml_rust::{Yaml, YamlLoader};

fn main() {
    // Parse arguments
    let matches = parse::get_args();
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

fn load_file(filename: &str) -> Yaml {
    if let Ok(mut file) = File::open(filename) {
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_err() {
            println!("Failed to read from file {}", filename);
            process::exit(1);
        }
        match YamlLoader::load_from_str(&buf) {
            Ok(mut yaml) => {yaml.swap_remove(0)}
            Err(_) => {
                println!("Failed to parse file {}", filename);
                process::exit(1);
            }
        }
    } else {
        // No file
        Yaml::Null
    }
}

fn deploy(matches: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Deploy args: {:?}", matches);
    let filename = matches.value_of("config").unwrap();
    let configuration = load_file(filename);
    verb!(verbosity, 2, "configuration: {:?}", configuration);
}

fn config(matches: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Config args: {:?}", matches);
}
