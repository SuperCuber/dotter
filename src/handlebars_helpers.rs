use std::process::{Command, Stdio};

use config::{Files, Helpers, Variables};

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError};

use meval;
use toml::value::{Table, Value};

fn math_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
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
    h: &Helper,
    handlebars: &Handlebars,
    ctx: &Context,
    _: &mut RenderContext,
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
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
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

    let status = is_executable(&executable).map_err(|e| RenderError::from_error("is_executable", e))?;
    if status {
        out.write("true")?;
    }
    // writing anything other than an empty string is considered truthy

    Ok(())
}

#[cfg(windows)]
fn is_executable(name: &str) -> Result<bool, std::io::Error> {
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

pub fn register_rust_helpers(handlebars: &mut Handlebars) {
    handlebars.register_helper("math", Box::new(math_helper));
    handlebars.register_helper("include_template", Box::new(include_template_helper));
    handlebars.register_helper("is_executable", Box::new(is_executable_helper));
}

pub fn register_script_helpers(handlebars: &mut Handlebars, helpers: Helpers) {
    debug!("Registering script helpers...");
    for (helper_name, helper_path) in helpers {
        if let Err(e) = handlebars.register_script_helper_file(&helper_name, &helper_path) {
            error!(
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

pub fn add_dotter_variable(variables: &mut Variables, files: &Files, packages: &[String]) {
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

    variables.insert("dotter".into(), dotter.into());
}
