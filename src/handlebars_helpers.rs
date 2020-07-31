use config::Helpers;

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError};

use meval;

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
    for (helper_name, helper_path) in helpers {
        if let Err(e) = handlebars.register_script_helper_file(&helper_name, &helper_path) {
            error!(
                "Coudln't register helper script {} at path {:?} because {}",
                helper_name, helper_path, e
            );
            ::std::process::exit(1);
        }
    }
}
