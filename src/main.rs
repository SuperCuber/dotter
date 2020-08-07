#[cfg(windows)]
extern crate dunce;

extern crate clap;
extern crate env_logger;
extern crate handlebars;
#[macro_use]
extern crate log;
extern crate meval;
extern crate serde;
extern crate shellexpand;
extern crate structopt;
extern crate toml;

mod args;
mod config;
mod deploy;
mod filesystem;
mod handlebars_helpers;

use std::env;
use std::process;

fn main() {
    // Parse arguments
    let opt = args::get_options();

    if opt.act {
        env_logger::init();
    } else {
        env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    debug!("Loaded options: {:?}", opt);

    // Change dir
    info!("Changing directory to {:?}", &opt.directory);
    if env::set_current_dir(&opt.directory).is_err() {
        error!("Couldn't set current directory to {:?}", &opt.directory);
        process::exit(1);
    }

    deploy::deploy(opt)
}
