use crate::filesystem;
use std::path::PathBuf;

use clap::{Parser, Subcommand, CommandFactory, FromArgMatches};
use clap_complete::Shell;

type ForceType = bool;
type NoConfirmType = bool;
type QuietType= bool;
type DiffContextLinesType= usize;
type VerbosityType = u8;

macro_rules! merge_setting {
    ($opt:expr, $matches:expr, $repo:expr, $global:expr, [ $($field:ident),+ ]) => {
        $(
        if $matches.value_source(stringify!($field)) != Some(clap::parser::ValueSource::CommandLine) {
            $opt.$field = $repo.$field.or($global.$field).unwrap_or($opt.$field);
        }
        )+
    };
}

/// A small dotfile manager.
#[derive(Debug, Parser, Default, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
    /// Location of the global configuration
    #[clap(
        short,
        long,
        value_parser,
        default_value = ".dotter/global.toml",
        global = true
    )]
    pub global_config: PathBuf,

    /// Location of the local configuration
    #[clap(
        short,
        long,
        value_parser,
        default_value = ".dotter/local.toml",
        global = true
    )]
    pub local_config: PathBuf,

    /// Location of cache file
    #[clap(long, value_parser, default_value = ".dotter/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[clap(long, value_parser, default_value = ".dotter/cache")]
    pub cache_directory: PathBuf,

    /// Location of optional pre-deploy hook
    #[clap(long, value_parser, default_value = ".dotter/pre_deploy.sh")]
    pub pre_deploy: PathBuf,

    /// Location of optional post-deploy hook
    #[clap(long, value_parser, default_value = ".dotter/post_deploy.sh")]
    pub post_deploy: PathBuf,

    /// Location of optional pre-undeploy hook
    #[clap(long, value_parser, default_value = ".dotter/pre_undeploy.sh")]
    pub pre_undeploy: PathBuf,

    /// Location of optional post-undeploy hook
    #[clap(long, value_parser, default_value = ".dotter/post_undeploy.sh")]
    pub post_undeploy: PathBuf,

    /// Dry run - don't do anything, only print information.
    /// Implies -v at least once
    #[clap(short = 'd', long = "dry-run", global = true)]
    pub dry_run: bool,

    /// Verbosity level - specify up to 3 times to get more detailed output.
    /// Specifying at least once prints the differences between what was before and after Dotter's run
    #[clap(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbosity: VerbosityType,

    /// Quiet - only print errors
    #[clap(short, long, value_parser, global = true)]
    pub quiet: QuietType,

    /// Force - instead of skipping, overwrite target files if their content is unexpected.
    /// Overrides --dry-run.
    #[clap(short, long, value_parser, global = true)]
    pub force: ForceType,

    /// Assume "yes" instead of prompting when removing empty directories
    #[clap(short = 'y', long = "noconfirm", global = true)]
    pub noconfirm: NoConfirmType,

    /// Take standard input as an additional files/variables patch, added after evaluating
    /// `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch.
    #[clap(short, long, value_parser, global = true)]
    pub patch: bool,

    /// Amount of lines that are printed before and after a diff hunk.
    #[clap(long, value_parser, default_value = "3")]
    pub diff_context_lines: DiffContextLinesType,

    #[clap(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Subcommand, Default)]
pub enum Action {
    /// Deploy the files to their respective targets. This is the default subcommand.
    #[default]
    Deploy,

    /// Delete all deployed files from their target locations.
    /// Note that this operates on all files that are currently in cache.
    Undeploy,

    /// Initialize global.toml with a single package containing all the files in the current
    /// directory pointing to a dummy value and a local.toml that selects that package.
    Init,

    /// Run continuously, watching the repository for changes and deploying as soon as they
    /// happen. Can be ran with `--dry-run`
    #[cfg(feature = "watch")]
    Watch,

    /// Generate shell completions
    GenCompletions {
        /// Set the shell for generating completions [values: bash, elvish, fish, powerShell, zsh]
        #[clap(long, short)]
        shell: Shell,

        /// Set the out directory for writing completions file
        #[clap(long)]
        to: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct DotterSettings {
    pub repo: Option<PathBuf>,
    pub force: Option<ForceType>,
    pub noconfirm: Option<NoConfirmType>,
    pub quiet: Option<QuietType>,
    pub diff_context_lines: Option<DiffContextLinesType>,
    pub verbosity: Option<VerbosityType>,
}

fn load_settings_file(path: &std::path::Path) -> Option<DotterSettings> {
    filesystem::load_file(path).unwrap_or_else(|e| {
        log::warn!("Failed to load settings file {:?}: {:#}", path, e);
        None
    })
}

pub fn get_options() -> Options {
    let matches = Options::command().get_matches();
    let mut opt = Options::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    //TODO: do you agree, mr. maintainer SuperCuber, with the decision of having the config file
    //come from whatever `dirs` considers the platform-specific dirs? namely: https://docs.rs/dirs/latest/dirs/fn.config_dir.html
    //or should we make it all be just ~/.config/dotter/dotter.toml?
    //first of all, i like that path for MacOS as well, and it also works for Windows, technically. `~/` resolved everywhere.
    let global_settings = dirs::config_dir()
        // dotter/dotter.toml
        .map(|d| d.join("dotter").join("dotter.toml"))
        .and_then(|p: std::path::PathBuf| load_settings_file(&p))
        .unwrap_or_default();

    if let Some(repo) = &global_settings.repo {
        if let Err(e) = std::env::set_current_dir(repo) {
            log::warn!("Failed to cd to repo {:?}: {}", repo, e);
        }
    }

    let repo_settings = load_settings_file(std::path::Path::new("dotter.toml")).unwrap_or_default(); //not
    //sure what the default here would be tho

    merge_setting!(
        opt,
        matches,
        repo_settings,
        global_settings,
        [force, noconfirm, quiet, diff_context_lines, verbosity]
    );

    if opt.dry_run {
        opt.verbosity = std::cmp::max(opt.verbosity, 1);
    }
    opt.verbosity = std::cmp::min(3, opt.verbosity);
    if opt.patch {
        opt.noconfirm = true;
    }
    opt
}
