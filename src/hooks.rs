use anyhow::{Context, Result};
use handlebars::Handlebars;

use std::path::Path;
use std::process::Command;

use crate::config::TemplateTarget;

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

    let target: TemplateTarget = std::env::temp_dir().join("dotter_temp").into();
    crate::actions::perform_template_deploy(
        location,
        &script_file,
        &target,
        &mut crate::filesystem::RealFilesystem::new(false),
        handlebars,
        variables,
    )
    .context("deploy script")?;

    debug!("Running script file");
    let mut child = if cfg!(windows) {
        Command::new(target.target)
            .spawn()
            .context("spawn batch file")?
    } else {
        use std::os::unix::fs::PermissionsExt;
        
        let permissions = target.target.metadata()?.permissions();
        if !target.target.is_dir() && permissions.mode() & 0o111 != 0 {
            Command::new(target.target)
                .spawn()
                .context("spawn script file")?
        } else {
            Command::new("sh")
                .arg(target.target)
                .spawn()
                .context("spawn shell")?
        }
    };

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    Ok(())
}
