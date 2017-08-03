use clap;
use parse;
use toml::value::Table;
use std::process;

pub fn config(
    matches: &clap::ArgMatches<'static>,
    specific: &clap::ArgMatches<'static>,
    verbosity: u64,
    act: bool,
) {
    verb!(verbosity, 3, "Config args: {:?}", matches);
    let filename = match (
        specific.occurrences_of("file"),
        specific.occurrences_of("variable"),
        specific.occurrences_of("secret"),
    ) {
        (1, 0, 0) => matches.value_of("files").unwrap(),
        (0, 1, 0) => matches.value_of("variables").unwrap(),
        (0, 0, 1) => matches.value_of("secrets").unwrap(),
        _ => unreachable!(),
    };
    verb!(verbosity, 1, "Operating on file {}", filename);

    let mut parsed: Table = or_err!(parse::load_file(filename));
    verb!(verbosity, 2, "Loaded data: {:?}", parsed);

    match (
        specific.occurrences_of("add"),
        specific.occurrences_of("remove"),
        specific.occurrences_of("display"),
    ) {
        (1, 0, 0) => {
            let mut pair = specific.values_of("add").unwrap();
            let key = String::from(pair.next().unwrap());
            let value = pair.next().unwrap();
            let value = ::toml::Value::String(String::from(value));
            verb!(
                verbosity,
                1,
                "Inserting {} -> {:?}.\nBefore: {}",
                key,
                value,
                pretty_print(&parsed)
            );
            if act {
                parsed.insert(key, value);
            }
            verb!(verbosity, 1, "After: {}", pretty_print(&parsed));
        }
        (0, 1, 0) => {
            let key = specific.value_of("remove").unwrap();
            verb!(
                verbosity,
                1,
                "Removing {}.\nBefore: {}",
                key,
                pretty_print(&parsed)
            );
            if act {
                parsed.remove(key);
            }
            verb!(verbosity, 1, "After: {:?}", pretty_print(&parsed));
        }
        (0, 0, 1) => {
            println!("{}", pretty_print(&parsed));
        }
        _ => unreachable!(),
    }

    or_err!(parse::save_file(filename, &parsed));
}

fn pretty_print(table: &Table) -> String {
    let mut output = String::new();
    for pair in table {
        output.push_str(pair.0);
        output.push_str(" = ");
        output.push_str(pair.1.as_str().unwrap());
        output.push('\n');
    }
    output.pop(); // Last \n
    output
}
