extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate serde;
extern crate shellexpand;
extern crate structopt;
extern crate toml;

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

    if opt.global_options.act {
        env_logger::init();
    } else {
        env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    debug!("Loaded options: {:?}", opt);

    // Change dir
    let dir = &opt.global_options.directory;
    info!("Changing directory to {:?}", dir);
    if env::set_current_dir(dir).is_err() {
        error!("Couldn't set current directory to {:?}", dir);
        process::exit(1);
    }

    match opt.command {
        args::Command::Deploy {
            cache,
            cache_directory,
        } => deploy::deploy(&cache_directory, cache, opt.global_options),
        args::Command::Config { target, action } => {
            config::config(target, action, opt.global_options)
        }
    }
}
