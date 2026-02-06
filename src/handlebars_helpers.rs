use anyhow::{Context as AnyhowContext, Result};

use handlebars::{
    Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderErrorReason,
};
use toml::value::{Table, Value};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(feature = "scripting")]
use crate::config::Helpers;
use crate::config::{Configuration, Files, Variables};

pub fn create_new_handlebars<'b>(config: &mut Configuration) -> Result<Handlebars<'b>> {
    debug!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(str::to_string); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    register_rust_helpers(&mut handlebars);

    #[cfg(feature = "scripting")]
    register_script_helpers(&mut handlebars, &config.helpers);

    add_dotter_variable(&mut config.variables, &config.files, &config.packages);
    filter_files_condition(&handlebars, &config.variables, &mut config.files)
        .context("filter files based on `if` field")?;
    trace!("Handlebars instance: {:#?}", handlebars);
    Ok(handlebars)
}

fn filter_files_condition(
    handlebars: &Handlebars<'_>,
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

fn eval_condition(
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    condition: &str,
) -> Result<bool> {
    // extra { for format!()
    let condition = format!("{{{{#if {condition} }}}}true{{{{/if}}}}");
    let rendered = handlebars
        .render_template(&condition, variables)
        .context("")?;
    Ok(rendered == "true")
}

fn math_helper(
    h: &Helper<'_>,
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

    out.write(
        &evalexpr::eval(&expression)
            .map_err(|e| {
                RenderErrorReason::Other(format!(
                    "Cannot evaluate expression {expression} because {e}"
                ))
            })?
            .to_string(),
    )?;
    Ok(())
}

fn include_template_helper(
    h: &Helper<'_>,
    handlebars: &Handlebars<'_>,
    ctx: &Context,
    rc: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let path = params
        .next()
        .ok_or(RenderErrorReason::ParamNotFoundForIndex(
            "include_template",
            0,
        ))?
        .render();
    if params.next().is_some() {
        return Err(RenderErrorReason::Other(
            "include_template: More than one parameter given".to_owned(),
        )
        .into());
    }

    let included_file =
        std::fs::read_to_string(path).map_err(|e| RenderErrorReason::NestedError(Box::new(e)))?;
    let rendered_file = handlebars
        .render_template_with_context(&included_file, rc.context().as_deref().unwrap_or(ctx))
        .map_err(|e| RenderErrorReason::NestedError(Box::new(e)))?;

    out.write(&rendered_file)?;

    Ok(())
}

fn is_executable_helper(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let executable = params
        .next()
        .ok_or(RenderErrorReason::ParamNotFoundForIndex("is_executable", 0))?
        .render();
    if params.next().is_some() {
        return Err(RenderErrorReason::Other(
            "is_executable: More than one parameter given".to_owned(),
        )
        .into());
    }

    let status = is_executable(&executable)
        .map_err(|e| RenderErrorReason::Other(format!("failed to run is_executable: {e}")))?;
    if status {
        out.write("true")?;
    }
    // writing anything other than an empty string is considered truthy

    Ok(())
}

fn command_success_helper(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let command = params
        .next()
        .ok_or(RenderErrorReason::ParamNotFoundForIndex(
            "command_success",
            0,
        ))?
        .render();
    if params.next().is_some() {
        return Err(RenderErrorReason::Other(
            "command_success: More than one parameter given".to_owned(),
        )
        .into());
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
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let mut params = h.params().iter();
    let command = params
        .next()
        .ok_or(RenderErrorReason::ParamNotFoundForIndex(
            "command_success",
            0,
        ))?
        .render();
    if params.next().is_some() {
        return Err(RenderErrorReason::Other(
            "command_success: More than one parameter given".to_owned(),
        )
        .into());
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
fn is_executable(name: &str) -> Result<bool> {
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
        .context("run `where` command")
}

#[cfg(unix)]
fn is_executable(name: &str) -> Result<bool> {
    Command::new("which")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .context("run `which` command")
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

#[cfg(feature = "scripting")]
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

fn add_dotter_variable(
    variables: &mut Variables,
    files: &Files,
    packages: &BTreeMap<String, bool>,
) {
    let mut dotter = Table::new();
    dotter.insert(
        "packages".into(),
        Value::Table(
            packages
                .iter()
                .map(|(p, e)| (p.to_string(), Value::Boolean(*e)))
                .collect(),
        ),
    );
    dotter.insert("files".into(), files_as_toml(files));
    dotter.insert(
        "os".into(),
        (if cfg!(windows) { "windows" } else { "unix" }).into(),
    );

    // New better way
    dotter.insert("unix".into(), Value::Boolean(cfg!(unix)));
    dotter.insert(
        "windows".into(),
        Value::Boolean(cfg!(target_os = "windows")),
    );
    dotter.insert("linux".into(), Value::Boolean(cfg!(target_os = "linux")));
    dotter.insert("macos".into(), Value::Boolean(cfg!(target_os = "macos")));

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
    use crate::config::Settings;

    use super::*;

    #[test]
    fn eval_condition_simple() {
        let mut config = Configuration {
            files: Files::new(),
            variables: toml::map::Map::from_iter([("foo".into(), 2.into())]),
            #[cfg(feature = "scripting")]
            helpers: Helpers::new(),
            packages: maplit::btreemap! { "default".into() => true, "disabled".into() => false },
            recurse: true,
            settings: Settings::default(),
        };
        let handlebars = create_new_handlebars(&mut config).unwrap();

        assert!(eval_condition(&handlebars, &config.variables, "foo").unwrap());
        assert!(!eval_condition(&handlebars, &config.variables, "bar").unwrap());
        assert!(eval_condition(&handlebars, &config.variables, "dotter.packages.default").unwrap());
        assert!(
            !eval_condition(&handlebars, &config.variables, "dotter.packages.nonexist").unwrap()
        );
        assert!(
            !eval_condition(
                &handlebars,
                &config.variables,
                "(and true dotter.packages.disabled)"
            )
            .unwrap()
        );
    }

    #[test]
    fn eval_condition_helpers() {
        let mut config = Configuration {
            files: Files::new(),
            variables: Variables::new(),
            #[cfg(feature = "scripting")]
            helpers: Helpers::new(),
            packages: BTreeMap::new(),
            recurse: true,
            settings: Settings::default(),
        };
        let handlebars = create_new_handlebars(&mut config).unwrap();

        assert!(
            !eval_condition(
                &handlebars,
                &config.variables,
                "(is_executable \"no_such_executable_please\")"
            )
            .unwrap()
        );
        assert!(
            eval_condition(&handlebars, &config.variables, "(eq (math \"5+5\") \"10\")").unwrap()
        );
    }
}
