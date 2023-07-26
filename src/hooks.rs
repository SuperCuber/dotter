use anyhow::{Context, Result};
use handlebars::Handlebars;

use std::path::Path;
use std::process::Child;
use std::process::Command;

use crate::filesystem::{Filesystem, RealFilesystem};

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
    let mut fs = RealFilesystem::new(false);
    crate::actions::perform_template_deploy(
        location,
        &script_file,
        None,
        &mut fs,
        handlebars,
        variables,
    )
    .context("deploy script")?;
    fs.copy_permissions(location, &script_file, &None)
        .context("copy permissions from source to cache")?;

    debug!("Running script file");
    let mut child = run_script_file(&script_file)?;

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    Ok(())
}

#[cfg(unix)]
fn run_script_file(script: &Path) -> Result<Child> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = script.metadata()?.permissions();
    if !script.is_dir() && permissions.mode() & 0o111 != 0 {
        Command::new(script).spawn().context("spawn script file")
    } else {
        Command::new("sh")
            .arg(script)
            .spawn()
            .context("spawn shell")
    }
}

#[cfg(windows)]
fn run_script_file(script: &Path) -> Result<Child> {
    Command::new(script).spawn().context("spawn batch file")
}
