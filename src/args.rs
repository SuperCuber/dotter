use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "Dotter", about = "A small dotfile manager.")]
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
    #[structopt(long = "dry_run", parse(from_flag = std::ops::Not::not))]
    pub act: bool,

    /// Location of cache file
    #[structopt(long, default_value = "dotter_settings/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[structopt(long, default_value = "dotter_settings/cache")]
    pub cache_directory: PathBuf,
}

pub fn get_options() -> Options {
    Options::from_args()
}
