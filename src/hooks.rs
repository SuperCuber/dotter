use anyhow::{Context, Result};
use handlebars::Handlebars;

use std::path::Path;

pub(crate) fn run_hook(
    location: &Path,
    handlebars: &Handlebars,
    variables: &crate::config::Variables,
) -> Result<()> {
    let script = std::fs::read_to_string(location).context("read script file")?;
    let script = handlebars
        .render_template(&script, variables)
        .context("render script as template")?;

    Ok(())
}
