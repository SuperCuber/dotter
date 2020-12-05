use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "Dotter")]
/// A small dotfile manager.
/// Note that flags and options have to come BEFORE subcommands.
pub struct Options {
    /// Location of the global configuration
    #[structopt(short, long, default_value = ".dotter/global.toml")]
    pub global_config: PathBuf,

    /// Location of the local configuration
    #[structopt(short, long, default_value = ".dotter/local.toml")]
    pub local_config: PathBuf,

    /// Dry run - don't do anything, only print information.
    /// Implies RUST_LOG=info unless specificed otherwise.
    #[structopt(long = "dry-run", parse(from_flag = std::ops::Not::not))]
    pub act: bool,

    /// Location of cache file
    #[structopt(long, default_value = ".dotter/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[structopt(long, default_value = ".dotter/cache")]
    pub cache_directory: PathBuf,

    /// Force - instead of skipping, overwrite target files if their content is unexpected.
    /// Overrides --dry-run and implies RUST_LOG=warn unless specified otherwise.
    #[structopt(long)]
    pub force: bool,

    #[structopt(subcommand)]
    pub action: Option<Action>,

    /// Assume "yes" instead of prompting when removing empty directories
    #[structopt(short = "y", long = "noconfirm", parse(from_flag = std::ops::Not::not))]
    pub interactive: bool,
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

    /// Run continuously, watching the repository for changes and running a subcommand as soon as they
    /// happen.
    Watch {
        #[structopt(subcommand)]
        action: Option<WatchedAction>,
    },

    /// Print the differences that will result when running a deploy (in templates only).
    /// Does not actually execute the deploy
    Diff,
}

#[derive(Debug, Clone, Copy, StructOpt)]
pub enum WatchedAction {
    /// Deploys when a change is detected
    Deploy,

    /// Shows diff when a change is detected
    Diff,
}

impl Default for Action {
    fn default() -> Self {
        Action::Deploy
    }
}

impl Default for WatchedAction {
    fn default() -> Self {
        WatchedAction::Deploy
    }
}

pub fn get_options() -> Options {
    let mut opt = Options::from_args();
    if opt.force {
        opt.act = true;
    }
    opt
}
