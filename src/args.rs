use std::path::PathBuf;

use structopt::StructOpt;
use clap;

#[derive(Debug, StructOpt)]
#[structopt(name = "Dotter", about = "A small dotfile manager.")]
pub struct Options {
    #[structopt(flatten)]
    pub global_options: GlobalOptions,

    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(Debug, StructOpt)]
pub struct GlobalOptions {
    /// Do all operations relative to this directory.
    #[structopt(short, long, default_value = ".")]
    pub directory: PathBuf,

    /// Location of the files configuration
    #[structopt(short, long, default_value="dotter_settings/files.toml")]
    pub files: PathBuf,

    /// Location of the variables configuration
    #[structopt(short="V", long, default_value="dotter_settings/variables.toml")]
    pub variables: PathBuf,

    /// Location of the secrets configuration - doesn't have to exist
    #[structopt(short, long, default_value="dotter_settings/secrets.toml")]
    pub secrets: PathBuf,

    /// Print information about what's being done. Repeat for more information.
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u32,

    /// Dry run - don't do anything, only print information. Implies -v at least once.
    #[structopt(long = "dry_run", parse(from_flag = std::ops::Not::not))]
    pub act: bool,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    /// Copy all files to their configured locations.
    Deploy {
        /// Don't use a cache (caching is used in order to avoid touching files that didn't change)
        #[structopt(long="no-cache", parse(from_flag = std::ops::Not::not))]
        cache: bool,

        /// Directory to cache into.
        #[structopt(short = "d", long, default_value = "dotter_cache")]
        cache_directory: PathBuf,
    },

    /// Configure files/variables/secrets.
    #[structopt(group = clap::ArgGroup::with_name("target").required(true),
                group = clap::ArgGroup::with_name("action").required(true))]
    Config {
        #[structopt(flatten)]
        target: Target,
        #[structopt(flatten)]
        action: Action,
    }
}

#[derive(Debug, StructOpt)]
pub struct Target {
    /// Operate on files.
    #[structopt(short, long, group="target")]
    file: bool,

    /// Operate on variables.
    #[structopt(short, long, group="target")]
    variable: bool,

    /// Operate on secrets.
    #[structopt(short, long, group="target")]
    secret: bool,
}

pub enum TargetType {
    File,
    Variable,
    Secret,
}

impl Target {
    pub fn as_type(&self) -> TargetType {
        match *self {
            Target { file: true, variable: false, secret: false } => TargetType::File,
            Target { file: false, variable: true, secret: false } => TargetType::Variable,
            Target { file: false, variable: false, secret: true } => TargetType::Secret,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, StructOpt)]
pub struct Action {
    /// In case of file, add file -> target entry,
    /// in case of variable/secret, add key -> value entry.
    #[structopt(short, long, group="action", value_names=&["from", "to"])]
    add: Option<Vec<String>>,

    /// Remove a file or variable from configuration.
    #[structopt(short, long, group="action")]
    remove: Option<String>,

    /// Display the configuration.
    #[structopt(short, long, group="action")]
    display: bool,
}

pub enum ActionEnum {
    Add {
        from: String,
        to: String,
    },
    Remove(String),
    Display,
}

impl Action {
    pub fn as_enum(self) -> ActionEnum {
        match self {
            Action { add: Some(mut v), remove: None, display: false } => {
                let from = v.swap_remove(0);
                let to = v.swap_remove(0);
                ActionEnum::Add { from, to }
            },
            Action { add: None, remove: Some(s), display: false } => ActionEnum::Remove(s),
            Action { add: None, remove: None, display: true }     => ActionEnum::Display,
            _ => unreachable!(),
        }
    }
}

pub fn get_options() -> Options {
    let mut opt = Options::from_args();

    // Do the "implies" relation between verbose and dry_run
    opt.global_options.verbose = if opt.global_options.act {
        opt.global_options.verbose
    } else {
        std::cmp::max(1, opt.global_options.verbose)
    };

    opt
}
