#[path = "src/args.rs"]
mod args;

use self::args::Options;
use clap::CommandFactory;
use clap_complete::generate_to;
use clap_complete::Shell::*;
use clap_complete_nushell::Nushell;
use std::io;

fn main() -> io::Result<()> {
    let cmd = &mut Options::command();
    let name = "dotter";
    let dir = "completions";

    generate_to(Bash, cmd, name, dir)?;
    generate_to(Zsh, cmd, name, dir)?;
    generate_to(Elvish, cmd, name, dir)?;
    generate_to(Fish, cmd, name, dir)?;
    generate_to(PowerShell, cmd, name, dir)?;
    generate_to(Nushell, cmd, name, dir)?;

    Ok(())
}
