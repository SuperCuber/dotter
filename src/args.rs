use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "Dotter")]
/// A small dotfile manager.
pub struct Options {
    /// Location of the global configuration
    #[structopt(short, long, default_value = ".dotter/global.toml", global = true)]
    pub global_config: PathBuf,

    /// Location of the local configuration
    #[structopt(short, long, default_value = ".dotter/local.toml", global = true)]
    pub local_config: PathBuf,

    /// Location of cache file
    #[structopt(long, default_value = ".dotter/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[structopt(long, default_value = ".dotter/cache")]
    pub cache_directory: PathBuf,

    /// Dry run - don't do anything, only print information.
    /// Implies -v at least once
    #[structopt(short = "d", long = "dry-run", parse(from_flag = std::ops::Not::not), global = true)]
    pub act: bool,

    /// Verbosity level - specify up to 3 times to get more detailed output.
    /// Specifying at least once prints the differences between what was before and after Dotter's run
    #[structopt(short = "v", long = "verbose", parse(from_occurrences), global = true)]
    pub verbosity: u64,

    /// Quiet - only print errors
    #[structopt(short, long, global = true)]
    pub quiet: bool,

    /// Force - instead of skipping, overwrite target files if their content is unexpected.
    /// Overrides --dry-run.
    #[structopt(short, long, global = true)]
    pub force: bool,

    /// Assume "yes" instead of prompting when removing empty directories
    #[structopt(short = "y", long = "noconfirm", parse(from_flag = std::ops::Not::not), global = true)]
    pub interactive: bool,

    /// Take standard input as an additional files/variables patch, added after evaluating
    /// `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch.
    #[structopt(short, long, global = true)]
    pub patch: bool,

    /// Amount of lines that are printed before and after a diff hunk.
    #[structopt(long, default_value = "3")]
    pub diff_context_lines: usize,

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

    /// Run continuously, watching the repository for changes and deploying as soon as they
    /// happen. Can be ran with `--dry-run`
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
    if !opt.act {
        opt.verbosity = std::cmp::max(opt.verbosity, 1);
    }
    opt.verbosity = std::cmp::min(3, opt.verbosity);
    if opt.patch {
        opt.interactive = false;
    }
    opt
}
