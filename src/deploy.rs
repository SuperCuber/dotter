use clap;
use parse;

use toml::value::Table;

use std::fs;
use std::process;
use std::io::{Read, Write};

pub fn deploy(global: &clap::ArgMatches<'static>,
          specific: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {

    // Configuration
    verb!(verbosity, 1, "Loading configuration...");
    let (files, variables) = load_configuration(global, verbosity);

    // Cache
    let cache = global.occurrences_of("nocache") == 0;
    verb!(verbosity, 1, "Cache: {}", cache);
    let cache_directory = specific.value_of("cache_directory").unwrap();
    if cache {
        verb!(verbosity, 1, "Creating cache directory at {}", cache_directory);
        if act {
            if fs::create_dir_all(cache_directory).is_err() {
                println!("Failed to create cache directory.");
                process::exit(1);
            }
        }
    }

    // Deploy files
    for pair in files {
        println!("deploying {} -> {}", pair.0, pair.1);
        let from = parse_path(&pair.0);
        let to = parse_path(pair.1.as_str().unwrap());
        deploy_file(from, to, &variables, verbosity,
                    act, cache, cache_directory)
    }
}

fn deploy_file(from: &str, to: &str, variables: &Table,
               verbosity: u64, act: bool, cache: bool,
               cache_directory: &str) {
    if !cache { verb!(verbosity, 1, "Copying {} to {}", from, to); }
    if !act { return; }

    if cache {
        let to_cache = ""; // [TODO]: Get cache place
        deploy_file(from, to_cache, variables, verbosity,
                    act, false, "");
        verb!(verbosity, 1, "Copying {} to {}", to_cache, to);
        // TODO: create_dir_all for path.split(dst)[0] <28-05-17, Amit Gold> //
        if fs::copy(to_cache, to).is_err() {
            println!("Warning: Failed copying {} to {}",
                     to_cache, to);
        }
    } else {
        // TODO: create_dir_all(path.split(dst)[0]) <28-05-17, Amit Gold> //
        // Also fix this V
        // let mode = fs::metadata(src).unwrap();
        let mut content = String::new();
        if let Ok(mut f_from) = fs::File::open(from) {
            if f_from.read_to_string(&mut content).is_err() {
                println!("Warning: Couldn't read from {}", from);
                return;
            }
            content = substitute_variables(&mut content, variables);
        } else {
            println!("Warning: Failed to open {} for reading", from);
            return;
        }
        if let Ok(mut f_to) = fs::File::create(to) {
            if f_to.write(content.as_bytes()).is_err() {
                println!("Warning: Couldn't write to {}", to);
                return;
            }
        } else {
            println!("Warning: Failed to open {} for reading", to);
            return;
        }
    }

}

fn load_configuration(matches: &clap::ArgMatches<'static>,
              verbosity: u64) -> (Table, Table) {
    verb!(verbosity, 3, "Deploy args: {:?}", matches);

    // Load config
    let configuration: parse::Config = parse::load_file(
            matches.value_of("config")
            .unwrap()).unwrap();
    verb!(verbosity, 2, "Configuration: {:?}", configuration);

    // Load secrets
    let secrets: parse::Secrets = parse::load_file(
            matches.value_of("secrets")
            .unwrap()).unwrap();
    let mut secrets = match secrets.secrets {
        Some(secrets) => {secrets}
        None => {
            println!("Warning: No secrets section in secrets file.");
            Table::new()
        }
    };
    verb!(verbosity, 2, "Secrets: {:?}", secrets);

    // Get files
    let files = match configuration.files {
        Some(files) => { files }
        None => {
            println!("Warning: No files section in config file.");
            Table::new()
        }
    };

    verb!(verbosity, 2, "Files: {:?}", files);

    // Get variables and update with secrets
    let mut variables = match configuration.variables {
        Some(variables) => { variables }
        None => { Table::new() }
    };
    variables.append(&mut secrets); // Secrets is now empty

    verb!(verbosity, 2, "Variables: {:?}", variables);

    (files, variables)
}

fn parse_path(path: &str) -> &str {
    // TODO: implement this <27-05-17, Amit Gold> //
    path
}

fn substitute_variables(content: &mut str, variables: &Table) -> String {
    // TODO: implement this <28-05-17, Amit Gold> //
    (content, variables);
    String::from("")
}
