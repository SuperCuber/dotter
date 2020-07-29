extern crate clap;
extern crate serde;
extern crate toml;
extern crate ansi_term;
extern crate structopt;

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
    let opt = args::get_options();
    let verbosity = opt.global_options.verbose;

    verb!(verbosity, 3, "{:?}", opt);

    // Change dir
    let dir = &opt.global_options.directory;
    verb!(verbosity, 1, "Changing directory to {:?}", dir);
    if env::set_current_dir(dir).is_err() {
        println!("Error: Couldn't set current directory to {:?}", dir);
        process::exit(1);
    }

    match opt.command {
        args::Command::Deploy { cache, cache_directory } => deploy::deploy(&cache_directory, cache, opt.global_options),
        args::Command::Config { target, action } => config::config(target, action, opt.global_options),
    }
}
