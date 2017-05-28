use clap;
use parse;
use toml::value::Table;

pub fn config(matches: &clap::ArgMatches<'static>,
          specific: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Config args: {:?}", matches);
    // TODO: remove <28-05-17, Amit Gold> //
    (specific, act);
    let filename = match (specific.occurrences_of("file"),
                          specific.occurrences_of("variable"),
                          specific.occurrences_of("secret")) {
        (1, 0, 0) => {
            matches.value_of("files").unwrap()
        }
        (0, 1, 0) => {
            matches.value_of("variables").unwrap()
        }
        (0, 0, 1) => {
            matches.value_of("secrets").unwrap()
        }
        (_, _, _) => { unreachable!(); }
    };
    verb!(verbosity, 1, "Operating on file {}", filename);

    let parsed: Table = parse::load_file(filename).unwrap();
    verb!(verbosity, 2, "Loaded data: {:?}", parsed);
}
