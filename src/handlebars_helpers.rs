use anyhow::{Context as AnyhowContext, Result};

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError};
use toml::value::{Table, Value};

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::config::{Configuration, Files, Helpers, Variables};

pub fn create_new_handlebars<'b>(config: &mut Configuration) -> Result<Handlebars<'b>> {
    debug!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    register_rust_helpers(&mut handlebars);
    register_script_helpers(&mut handlebars, &config.helpers);
    add_dotter_variable(&mut config.variables, &config.files, &config.packages);
    filter_files_condition(&handlebars, &config.variables, &mut config.files)
        .context("filter files based on `if` field")?;
    trace!("Handlebars instance: {:#?}", handlebars);
    Ok(handlebars)
}

fn filter_files_condition(
    handlebars: &Handlebars,
    variables: &Variables,
    files: &mut Files,
) -> Result<()> {
    let filtered = std::mem::take(files)
        .into_iter()
        .map(|(source, target)| -> Result<Option<_>> {
            let condition = target.condition();
            Ok(if let Some(condition) = condition {
                if eval_condition(handlebars, variables, condition).context("")? {
                    Some((source, target))
                } else {
                    None
                }
            } else {
                Some((source, target))
            })
        })
        .collect::<Result<BTreeSet<Option<(PathBuf, _)>>>>()?
        .into_iter()
        .flatten()
        .collect();
    *files = filtered;
    Ok(())
}

fn eval_condition(handlebars: &Handlebars, variables: &Variables, condition: &str) -> Result<bool> {
    // extra { for format!()
    let condition = format!("{{{{#if {} }}}}true{{{{/if}}}}", condition);
    let rendered = handlebars
        .render_template(&condition, variables)
        .context("")?;
    Ok(rendered == "true")
}

fn math_helper(
    h: &Helper<'_, '_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let params = h
        .params()
        .iter()
        .map(|p| p.render())
        .collect::<Vec<String>>();
    let expression = params.join(" ");

    let parsed = expression.parse::<meval::Expr>().map_err(|e| {
        RenderError::new(format!(
            "Cannot parse math expression {} because {}",
            expression, e
        ))
    })?;

    out.write(
        &parsed
            .eval()
            .map_err(|e| {
                RenderError::new(format!(
                    "Cannot evaluate expression {} because {}",
                    expression, e
                ))
            })?
            .to_string(),
    )?;
    Ok(())
}

fn include_template_helper(
    h: &Helper<'_, '_>,
    handlebars: &Handlebars<'_>,
    ctx: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let path = params
        .next()
        .ok_or_else(|| RenderError::new("include_template: No path given"))?
        .render();
    if params.next().is_some() {
        return Err(RenderError::new(
            "include_template: More than one parameter given",
        ));
    }

    let included_file = std::fs::read_to_string(path)
        .map_err(|e| RenderError::from_error("include_template", e))?;
    let rendered_file = handlebars
        .render_template_with_context(&included_file, ctx)
        .map_err(|e| RenderError::from_error("include_template", e))?;

    out.write(&rendered_file)?;

    Ok(())
}

fn is_executable_helper(
    h: &Helper<'_, '_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let executable = params
        .next()
        .ok_or_else(|| RenderError::new("is_executable: No executable name given"))?
        .render();
    if params.next().is_some() {
        return Err(RenderError::new(
            "is_executable: More than one parameter given",
        ));
    }

    let status =
        is_executable(&executable).map_err(|e| RenderError::from_error("is_executable", e))?;
    if status {
        out.write("true")?;
    }
    // writing anything other than an empty string is considered truthy

    Ok(())
}

fn command_success_helper(
    h: &Helper<'_, '_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let command = params
        .next()
        .ok_or_else(|| RenderError::new("command_success: No executable name given"))?
        .render();
    if params.next().is_some() {
        return Err(RenderError::new(
            "command_success: More than one parameter given",
        ));
    }

    let status = os_shell()
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success();
    if status {
        out.write("true")?;
    }
    // writing anything other than an empty string is considered truthy

    Ok(())
}

fn command_output_helper(
    h: &Helper<'_, '_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let command = params
        .next()
        .ok_or_else(|| RenderError::new("command_success: No executable name given"))?
        .render();
    if params.next().is_some() {
        return Err(RenderError::new(
            "command_success: More than one parameter given",
        ));
    }

    let output = os_shell()
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // .stderr(Stdio::piped()) - probably not wanted
        .output()?;
    out.write(&String::from_utf8_lossy(&output.stdout))?;
    // writing anything other than an empty string is considered truthy

    Ok(())
}

#[cfg(windows)]
fn is_executable(name: &str) -> Result<bool, std::io::Error> {
    let name = if name.ends_with(".exe") {
        name.to_string()
    } else {
        format!("{}.exe", name)
    };

    Command::new("where")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
}

#[cfg(unix)]
fn is_executable(name: &str) -> Result<bool, std::io::Error> {
    Command::new("which")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
}

#[cfg(windows)]
fn os_shell() -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C");
    cmd
}

#[cfg(unix)]
fn os_shell() -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    cmd
}

fn register_rust_helpers(handlebars: &mut Handlebars<'_>) {
    handlebars_misc_helpers::register(handlebars);
    handlebars.register_helper("math", Box::new(math_helper));

    handlebars.register_helper("include_template", Box::new(include_template_helper));
    handlebars.register_helper("is_executable", Box::new(is_executable_helper));
    handlebars.register_helper("command_success", Box::new(command_success_helper));
    handlebars.register_helper("command_output", Box::new(command_output_helper));
}

fn register_script_helpers(handlebars: &mut Handlebars<'_>, helpers: &Helpers) {
    debug!("Registering script helpers...");
    for (helper_name, helper_path) in helpers {
        if let Err(e) = handlebars.register_script_helper_file(helper_name, helper_path) {
            warn!(
                "Coudln't register helper script {} at path {:?} because {}",
                helper_name, helper_path, e
            );
        }
    }
}

fn files_as_toml(files: &Files) -> Value {
    Value::Table(
        files
            .iter()
            .map(|(source, target)| {
                (
                    source.to_string_lossy().to_string(),
                    target.path().to_string_lossy().to_string().into(),
                )
            })
            .collect(),
    )
}

fn add_dotter_variable(variables: &mut Variables, files: &Files, packages: &[String]) {
    let mut dotter = Table::new();
    dotter.insert(
        "packages".into(),
        Value::Table(
            packages
                .iter()
                .map(|p| (p.to_string(), Value::Boolean(true)))
                .collect(),
        ),
    );
    dotter.insert("files".into(), files_as_toml(files));
    dotter.insert(
        "os".into(),
        (if cfg!(windows) { "windows" } else { "unix" }).into(),
    );
    dotter.insert(
        "current_dir".into(),
        Value::String(
            std::env::current_dir()
                .expect("get current dir")
                .to_string_lossy()
                .into(),
        ),
    );
    if let Ok(hostname) = hostname::get() {
        dotter.insert(
            "hostname".into(),
            Value::String(hostname.to_string_lossy().into()),
        );
    } else {
        warn!("Failed to get hostname, skipping dotter.hostname variable");
    }

    variables.insert("dotter".into(), dotter.into());
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn eval_condition_simple() {
        let mut config = Configuration {
            files: Files::new(),
            variables: maplit::btreemap! { "foo".into() => 2.into() },
            helpers: Helpers::new(),
            packages: vec!["default".into()],
            recurse: true,
        };
        let handlebars = create_new_handlebars(&mut config).unwrap();

        assert_eq!(
            eval_condition(&handlebars, &config.variables, "foo").unwrap(),
            true
        );
        assert_eq!(
            eval_condition(&handlebars, &config.variables, "bar").unwrap(),
            false
        );
        assert_eq!(
            eval_condition(&handlebars, &config.variables, "dotter.packages.default").unwrap(),
            true
        );
        assert_eq!(
            eval_condition(&handlebars, &config.variables, "dotter.packages.nonexist").unwrap(),
            false
        );
    }

    #[test]
    fn eval_condition_helpers() {
        let mut config = Configuration {
            files: Files::new(),
            variables: Variables::new(),
            helpers: Helpers::new(),
            packages: vec!["default".into()],
            recurse: true,
        };
        let handlebars = create_new_handlebars(&mut config).unwrap();

        assert_eq!(
            eval_condition(
                &handlebars,
                &config.variables,
                "(is_executable \"no_such_executable_please\")"
            )
            .unwrap(),
            false
        );
        assert_eq!(
            eval_condition(&handlebars, &config.variables, "(eq (math \"5+5\") \"10\")").unwrap(),
            true
        );
    }
}
