#[cfg(windows)]
extern crate dunce;

#[macro_use]
extern crate anyhow;
extern crate clap;
extern crate crossterm;
extern crate diff;
extern crate handlebars;
#[macro_use]
extern crate log;
extern crate meval;
#[macro_use]
extern crate serde;
extern crate shellexpand;
extern crate simplelog;
extern crate structopt;
#[macro_use]
extern crate thiserror;
extern crate toml;
extern crate watchexec;

mod args;
mod config;
mod deploy;
mod difference;
mod file_state;
mod filesystem;
mod handlebars_helpers;
mod init;
mod watch;

use anyhow::{Context, Result};

fn main() {
    match run() {
        Ok(success) if success => std::process::exit(0),
        Ok(_) => std::process::exit(1),
        Err(e) => {
            display_error(e);
            std::process::exit(1);
        }
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

/// Returns true if program should exit with success status
fn run() -> Result<bool> {
    // Parse arguments
    let opt = args::get_options();

    use simplelog::LevelFilter;

    simplelog::TermLogger::init(
        if opt.quiet {
            LevelFilter::Off
        } else {
            match opt.verbosity {
                0 => LevelFilter::Warn,
                1 => LevelFilter::Info,
                2 => LevelFilter::Debug,
                3 => LevelFilter::Trace,
                _ => unreachable!(),
            }
        },
        simplelog::ConfigBuilder::new()
            .set_time_level(LevelFilter::Off)
            .set_location_level(LevelFilter::Debug)
            .set_target_level(LevelFilter::Off)
            .set_thread_level(LevelFilter::Off)
            .set_level_padding(simplelog::LevelPadding::Left)
            .add_filter_allow("dotter".into())
            .build(),
        simplelog::TerminalMode::Mixed,
    )
    .unwrap();

    trace!("Loaded options: {:#?}", opt);

    match opt.action.unwrap_or_default() {
        args::Action::Deploy => {
            debug!("Deploying...");
            if deploy::deploy(&opt).context("deploy")? {
                // An error occurred
                return Ok(false);
            }
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

    Ok(true)
}
