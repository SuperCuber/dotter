use anyhow::{Context, Result};
use crossterm::style::Colorize;
use diff;
use handlebars::Handlebars;

use std::{collections::BTreeMap, fs};

use args::Options;
use config::{self, Variables};
use deploy;
use file_state;
use handlebars_helpers;

pub fn diff(opt: &Options) -> Result<()> {
    let mut config = config::load_configuration(&opt.local_config, &opt.global_config)
        .context("get a configuration")?;
    let cache = config::load_cache(&opt.cache_file)
        .context("get cache")?
        .unwrap_or_default();
    let state = deploy::file_state_from_configuration(&config, &cache, &opt.cache_directory)
        .context("get file state")?;

    let (new_symlinks, new_templates) = state.new_files();
    let (deleted_symlinks, deleted_templates) = state.deleted_files();
    let (_old_symlinks, old_templates) = state.old_files();

    for new_symlink in new_symlinks {
        println!("{}{}", "[+] ".green(), new_symlink);
    }
    for new_template in new_templates {
        println!("{}{}", "[+] ".green(), new_template);
    }

    for deleted_symlink in deleted_symlinks {
        println!("{}{}", "[-] ".red(), deleted_symlink);
    }
    for deleted_template in deleted_templates {
        println!("{}{}", "[-] ".red(), deleted_template);
    }

    info!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, &config.helpers);
    handlebars_helpers::add_dotter_variable(&mut config.variables, &config.files, &config.packages);
    trace!("Handlebars instance: {:#?}", handlebars);

    let diffs: Result<BTreeMap<_, _>> = old_templates
        .into_iter()
        .map(|t| {
            let diff = generate_diff(&t, &handlebars, &config.variables);
            diff.map(|d| (t.clone(), d))
                .with_context(|| format!("generate diff for {}", t))
        })
        .collect();
    let diffs = diffs.context("generate diffs")?;

    for (template, diff) in diffs.into_iter().filter(diff_nonempty) {
        println!("\n{}{}", "----- ".blue(), template.to_string().blue());

        print_diff(diff);
    }

    Ok(())
}

fn generate_diff(
    template: &file_state::TemplateDescription,
    handlebars: &Handlebars,
    variables: &Variables,
) -> Result<Vec<diff::Result<String>>> {
    let file_contents =
        fs::read_to_string(&template.source).context("read template source file")?;
    let file_contents = template.apply_actions(file_contents);
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    let target_contents =
        fs::read_to_string(&template.target.target).context("read template target file")?;

    let diff_result = diff::lines(&target_contents, &rendered);

    Ok(diff_result.into_iter().map(to_owned_diff_result).collect())
}

fn to_owned_diff_result(from: diff::Result<&str>) -> diff::Result<String> {
    match from {
        diff::Result::Left(s) => diff::Result::Left(s.to_string()),
        diff::Result::Right(s) => diff::Result::Right(s.to_string()),
        diff::Result::Both(s1, s2) => diff::Result::Both(s1.to_string(), s2.to_string()),
    }
}

fn diff_nonempty<T>(diff: &(T, Vec<diff::Result<String>>)) -> bool {
    let diff = &diff.1;
    for line in diff {
        match line {
            diff::Result::Both(..) => {}
            _ => {
                return true;
            }
        }
    }
    false
}

fn print_diff(diff: Vec<diff::Result<String>>) {
    let minus = "-".red();
    let plus = "+".green();

    for line in diff {
        match line {
            diff::Result::Left(l) => println!("{}{}", minus, l.red()),
            diff::Result::Both(l, _) => println!(" {}", l),
            diff::Result::Right(r) => println!("{}{}", plus, r.green()),
        }
    }
}
