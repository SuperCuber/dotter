use anyhow::{Context, Result};
use crossterm::style::Colorize;
use diff;
use handlebars::Handlebars;

use std::fs;

use config::Variables;
use file_state;

pub fn generate_diff(
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

pub fn diff_nonempty(diff: &Vec<diff::Result<String>>) -> bool {
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

pub fn print_diff(diff: Vec<diff::Result<String>>) {
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
