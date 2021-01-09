use anyhow::{Context, Result};
use handlebars::Handlebars;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) fn run_hook(
    location: &Path,
    handlebars: &Handlebars,
    variables: &crate::config::Variables,
) -> Result<()> {
    let script = match std::fs::read_to_string(location) {
        Ok(script) => script,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => Err(e).context("read script file")?,
    };

    let script = handlebars
        .render_template(&script, variables)
        .context("render script as template")?;

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let mut child = Command::new(shell)
        .stdin(Stdio::piped())
        .spawn()
        .context("spawn shell")?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .context("write script to shell")?;

    anyhow::ensure!(
        child.wait().context("wait for child shell")?.success(),
        "subshell returned error"
    );

    Ok(())
}
