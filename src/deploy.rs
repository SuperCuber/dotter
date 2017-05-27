use clap;
use parse;

use toml::value::Table;

pub fn deploy(matches: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    let (files, variables) = load_configuration(matches, verbosity);

    if matches.occurences_of("")
}

pub fn load_configuration(matches: &clap::ArgMatches<'static>,
              verbosity: u64) -> (Table, Table) {
    verb!(verbosity, 3, "Deploy args: {:?}", matches);

    // Load configuration
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
