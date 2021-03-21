use anyhow::{Context, Result};
use handlebars::Handlebars;
use interprocess::unnamed_pipe::pipe;

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::Command;

pub(crate) fn run_hook(
    location: &Path,
    cache_dir: &Path,
    handlebars: &Handlebars,
    variables: &crate::config::Variables,
) -> Result<()> {
    if !location.exists() {
        debug!("Hook file at {:?} missing", location);
        return Ok(());
    }

    let mut script_file = cache_dir.join(location);
    if cfg!(windows) {
        script_file.set_extension("bat");
    }
    debug!("Rendering script {:?} -> {:?}", location, script_file);

    crate::actions::perform_template_deploy(
        location,
        &script_file,
        &std::env::temp_dir().join("dotter_temp").into(),
        &mut crate::filesystem::RealFilesystem::new(false),
        handlebars,
        variables,
    )
    .context("deploy script")?;

    debug!("Creating pipes ");
    let (pipe_writer, mut pipe_reader) = pipe()?;

    debug!("Running script file");
    let mut child = if cfg!(windows) {
        Command::new(script_file)
            .spawn()
            .context("spawn batch file")?
    } else {
        use std::os::unix::io::AsRawFd;

        let mut command_string = OsString::from(". ");
        command_string.push(script_file);
        command_string.push(format!("\nprintenv -0 >&{}", pipe_writer.as_raw_fd()));
        Command::new("sh")
            .arg("-c")
            .arg(&command_string)
            .spawn()
            .context("spawn shell")?
    };

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    {
        let _drop = pipe_writer;
    }

    // scoop up env vars
    let env_vars: Vec<(OsString, OsString)> = if cfg!(windows) {
        todo!();
    } else {
        let mut pipe_output = vec![];
        pipe_reader.read_to_end(&mut pipe_output)?;
        pipe_output.remove(pipe_output.len() - 1); // This is guarenteed to be a null character

        pipe_output
            .into_iter()
            .fold(vec![vec![]], |mut acc, c| {
                // Because we used printenv -0, everything is separated by null characters
                // Just need to separate on those
                if c == 0 {
                    acc.push(vec![]);
                } else {
                    acc.last_mut().unwrap().push(c);
                }
                acc
            })
            .into_iter()
            .map(|v| {
                // Now to separate the names from the values
                let envp = std::str::from_utf8(&v).unwrap();
                // posix compliance states that the seperator between env names and env values is the '=' character
                let i: Vec<_> = envp.splitn(2, "=").collect();
                (
                    i[0].to_string(),
                    i.get(1).map(|s| s.to_string()).unwrap_or(String::new()),
                )
            })
            .collect::<Vec<_>>()
    };

    Ok(())
}
