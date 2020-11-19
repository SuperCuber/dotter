use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "Dotter")]
/// A small dotfile manager.
/// Note that flags and options have to come BEFORE subcommands.
pub struct Options {
    /// Do all operations relative to this directory.
    #[structopt(short, long, default_value = ".")]
    pub directory: PathBuf,

    /// Location of the global configuration
    #[structopt(short, long, default_value = "dotter_settings/global.toml")]
    pub global_config: PathBuf,

    /// Location of the local configuration
    #[structopt(short, long, default_value = "dotter_settings/local.toml")]
    pub local_config: PathBuf,

    /// Dry run - don't do anything, only print information.
    /// Implies RUST_LOG=info unless specificed otherwise.
    #[structopt(long = "dry-run", parse(from_flag = std::ops::Not::not))]
    pub act: bool,

    /// Location of cache file
    #[structopt(long, default_value = "dotter_settings/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[structopt(long, default_value = "dotter_settings/cache")]
    pub cache_directory: PathBuf,

    /// Force - instead of skipping, overwrite target files if their content is unexpected.
    /// Overrides --dry-run and implies RUST_LOG=warn unless specified otherwise.
    #[structopt(long)]
    pub force: bool,

    #[structopt(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Copy, StructOpt)]
pub enum Action {
    /// Deploy the files to their respective targets. This is the default subcommand.
    Deploy,

    /// Delete all deployed files from their target locations.
    /// Note that this operates on all files that are currently in cache.
    Undeploy,

    /// Initialize global.toml with a single package containing all the files in the current
    /// directory pointing to a dummy value and a local.toml that selects that package.
    Init,

    /// Run continuously, watching the repository for changes and re-deploying as soon as they
    /// happen.
    Watch,
}

impl Default for Action {
    fn default() -> Self {
        Action::Deploy
    }
}

pub fn get_options() -> Options {
    let mut opt = Options::from_args();
    if opt.force {
        opt.act = true;
    }
    opt
}
