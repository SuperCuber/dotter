use clap;

pub fn config(matches: &clap::ArgMatches<'static>,
          specific: &clap::ArgMatches<'static>,
          verbosity: u64, act: bool) {
    verb!(verbosity, 3, "Config args: {:?}", matches);
    (specific, act);
}
