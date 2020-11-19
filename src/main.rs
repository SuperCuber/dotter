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
extern crate watchexec;

mod args;
mod config;
mod deploy;
mod filesystem;
mod handlebars_helpers;
mod init;
mod watch;

use std::env;

use anyhow::{Context, Result};

fn main() {
    if let Err(e) = run() {
        display_error(e);
        std::process::exit(1);
    }
}

pub(crate) fn display_error(error: anyhow::Error) {
    let mut chain = error.chain();
    let mut error_message = format!("Failed to {}\nCaused by:\n", chain.next().unwrap());

    for e in chain {
        error_message.push_str(&format!("    {}\n", e));
    }
    // Remove last \n
    error_message.pop();

    error!("{}", error_message);
}

fn run() -> Result<()> {
    // Parse arguments
    let opt = args::get_options();

    let log_level = if opt.act && opt.force {
        "warn"
    } else if opt.act && !opt.force {
        "error"
    } else if !opt.act && opt.force {
        unreachable!()
    } else {
        "info"
    };

    env_logger::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp(None)
        .format_module_path(false)
        .format_indent(Some(8))
        .init();

    trace!("Loaded options: {:#?}", opt);

    // Change dir
    info!("Changing directory to {:?}", &opt.directory);
    env::set_current_dir(&opt.directory)
        .with_context(|| format!("set current directory to {:?}", opt.directory))?;

    match opt.action.unwrap_or_default() {
        args::Action::Deploy => {
            debug!("Deploying...");
            deploy::deploy(&opt).context("deploy")?;
        }
        args::Action::Undeploy => {
            debug!("Un-Deploying...");
            deploy::undeploy(opt).context("undeploy")?;
        }
        args::Action::Init => {
            debug!("Initializing repo...");
            init::init(opt).context("initalize directory")?;
        }
        args::Action::Watch => {
            debug!("Watching...");
            watch::watch(opt).context("watch repository")?;
        }
    }

    Ok(())
}
