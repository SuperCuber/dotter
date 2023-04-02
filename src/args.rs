use std::io;
use std::path::PathBuf;

use clap::{Command, Parser, Subcommand, ValueEnum};
use clap_complete::Generator;

/// A small dotfile manager.
#[derive(Debug, Parser, Default, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
    /// Location of the global configuration
    #[arg(short, long, default_value = ".dotter/global.toml", global = true)]
    pub global_config: PathBuf,

    /// Location of the local configuration
    #[arg(short, long, default_value = ".dotter/local.toml", global = true)]
    pub local_config: PathBuf,

    /// Location of cache file
    #[arg(long, default_value = ".dotter/cache.toml")]
    pub cache_file: PathBuf,

    /// Directory to cache into.
    #[arg(long, default_value = ".dotter/cache")]
    pub cache_directory: PathBuf,

    /// Location of optional pre-deploy hook
    #[arg(long, default_value = ".dotter/pre_deploy.sh")]
    pub pre_deploy: PathBuf,

    /// Location of optional post-deploy hook
    #[arg(long, default_value = ".dotter/post_deploy.sh")]
    pub post_deploy: PathBuf,

    /// Location of optional pre-undeploy hook
    #[arg(long, default_value = ".dotter/pre_undeploy.sh")]
    pub pre_undeploy: PathBuf,

    /// Location of optional post-undeploy hook
    #[arg(long, default_value = ".dotter/post_undeploy.sh")]
    pub post_undeploy: PathBuf,

    /// Dry run - don't do anything, only print information.
    /// Implies -v at least once
    #[arg(short = 'd', long = "dry-run", global = true)]
    pub dry_run: bool,

    /// Verbosity level - specify up to 3 times to get more detailed output.
    /// Specifying at least once prints the differences between what was before and after Dotter's run
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbosity: u8,

    /// Quiet - only print errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Force - instead of skipping, overwrite target files if their content is unexpected.
    /// Overrides --dry-run.
    #[arg(short, long, global = true)]
    pub force: bool,

    /// Assume "yes" instead of prompting when removing empty directories
    #[arg(short = 'y', long = "noconfirm", global = true)]
    pub noconfirm: bool,

    /// Take standard input as an additional files/variables patch, added after evaluating
    /// `local.toml`. Assumes --noconfirm flag because all of stdin is taken as the patch.
    #[arg(short, long, global = true)]
    pub patch: bool,

    /// Amount of lines that are printed before and after a diff hunk.
    #[arg(long, default_value = "3")]
    pub diff_context_lines: usize,

    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Copy, Subcommand, Default)]
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
    Watch,

    /// Generate shell completions
    GenCompletions {
        /// Set the shell for generating completions [values: bash, elvish, fish, powershell, zsh, nushell]
        #[clap(long, short)]
        shell: Shell,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
    Nushell,
}

impl Generator for Shell {
    fn file_name(&self, name: &str) -> String {
        use clap_complete::Shell::*;
        use clap_complete_nushell::Nushell;
        match self {
            Self::Bash => Bash.file_name(name),
            Self::Elvish => Elvish.file_name(name),
            Self::Fish => Fish.file_name(name),
            Self::Powershell => PowerShell.file_name(name),
            Self::Zsh => Zsh.file_name(name),
            Self::Nushell => Nushell.file_name(name),
        }
    }

    fn generate(&self, cmd: &Command, buf: &mut dyn io::Write) {
        use clap_complete::Shell::*;
        use clap_complete_nushell::Nushell;
        match self {
            Self::Bash => Bash.generate(cmd, buf),
            Self::Elvish => Elvish.generate(cmd, buf),
            Self::Fish => Fish.generate(cmd, buf),
            Self::Powershell => PowerShell.generate(cmd, buf),
            Self::Zsh => Zsh.generate(cmd, buf),
            Self::Nushell => Nushell.generate(cmd, buf),
        }
    }
}

pub fn get_options() -> Options {
    let mut opt = Options::parse();
    if opt.dry_run {
        opt.verbosity = std::cmp::max(opt.verbosity, 1);
    }
    opt.verbosity = std::cmp::min(3, opt.verbosity);
    if opt.patch {
        opt.noconfirm = true;
    }
    opt
}
