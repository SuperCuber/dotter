use anyhow::{Context, Result};
use handlebars::Handlebars;
use interprocess::unnamed_pipe::pipe;

use std::ffi::{OsStr, OsString};
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

    for (name, value) in run_and_get_env(&script_file)? {
        std::env::set_var(name, value);
    }

    Ok(())
}

#[cfg(windows)]
fn run_and_get_env(script_file: &Path) -> Result<Vec<(OsString, OsString)>> {
    let mut child = Command::new(script_file)
        .spawn()
        .context("spawn batch file")?;

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    Ok(vec![])
}

#[cfg(unix)]
fn run_and_get_env(script_file: &Path) -> Result<Vec<(OsString, OsString)>> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::AsRawFd;

    debug!("Creating pipes ");
    let (pipe_writer, mut pipe_reader) = pipe()?;

    debug!("Running script file");

    let mut command_string = OsString::from(". ");
    command_string.push(script_file);
    command_string.push(format!("\nprintenv -0 >&{}", pipe_writer.as_raw_fd()));
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command_string)
        .spawn()
        .context("spawn shell")?;

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    {
        let _drop = pipe_writer;
    }

    // scoop up env vars
    let mut pipe_output = vec![];
    pipe_reader.read_to_end(&mut pipe_output)?;
    pipe_output.remove(pipe_output.len() - 1); // last char is a null character; make the split easier

    Ok(pipe_output
        .split(|c| c == &('\0' as u8)) // separate each char
        .map(|pair| pair.splitn(2, |c| c == &('=' as u8))) // posix compliance states that the seperator between env names and env values is the '=' character
        .flat_map(|mut i| {
            Some((
                OsStr::from_bytes(i.next()?).to_owned(),
                i.next()
                    .map(|s| OsStr::from_bytes(s))
                    .unwrap_or(OsStr::new(""))
                    .to_owned(),
            ))
        })
        .collect())
}
