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

pub fn register_rust_helpers(handlebars: &mut Handlebars) {
    handlebars.register_helper("math", Box::new(math_helper));
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
