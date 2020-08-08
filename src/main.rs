#[cfg(windows)]
extern crate dunce;

#[macro_use]
extern crate anyhow;
extern crate clap;
extern crate env_logger;
extern crate handlebars;
#[macro_use]
extern crate log;
extern crate meval;
#[macro_use]
extern crate serde;
extern crate shellexpand;
extern crate structopt;
#[macro_use]
extern crate thiserror;
extern crate toml;

mod args;
mod config;
mod deploy;
mod filesystem;
mod handlebars_helpers;

use std::env;

use anyhow::{Context, Result};

fn main() -> Result<()> {
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
    env::set_current_dir(&opt.directory)
        .with_context(|| format!("Failed to set current directory to {:?}", opt.directory))?;

    deploy::deploy(opt).context("Failed to deploy")?;
    Ok(())
}
