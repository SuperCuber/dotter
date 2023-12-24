use anyhow::{Context, Result};
use crossterm::style::Stylize;
use handlebars::Handlebars;

use std::cmp::{max, min};
use std::fs;
use std::path::Path;

use crate::config::{TemplateTarget, Variables};

pub type Diff = Vec<diff::Result<String>>;
pub type HunkDiff = Vec<(usize, usize, Diff)>;

pub fn print_template_diff(
    source: &Path,
    target: &TemplateTarget,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    diff_context_lines: usize,
) {
    if log_enabled!(log::Level::Info) {
        match generate_template_diff(source, target, handlebars, variables, true) {
            Ok(diff) => {
                if diff_nonempty(&diff) {
                    info!(
                        "{} template {:?} -> {:?}",
                        "[~]".yellow(),
                        source,
                        target.target
                    );
                    print_diff(&diff, diff_context_lines);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to generate diff for template {:?} -> {:?} on step: {}",
                    source, target.target, e
                );
            }
        }
    }
}

pub fn generate_template_diff(
    source: &Path,
    target: &TemplateTarget,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    source_to_target: bool,
) -> Result<Diff> {
    let file_contents = fs::read_to_string(source).context("read template source file")?;
    let file_contents = target.apply_actions(file_contents);
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    let target_contents =
        fs::read_to_string(&target.target).context("read template target file")?;

    let diff_result = if source_to_target {
        diff::lines(&target_contents, &rendered)
    } else {
        diff::lines(&rendered, &target_contents)
    };

    Ok(diff_result.into_iter().map(to_owned_diff_result).collect())
}

fn to_owned_diff_result(from: diff::Result<&str>) -> diff::Result<String> {
    match from {
        diff::Result::Left(s) => diff::Result::Left(s.to_string()),
        diff::Result::Right(s) => diff::Result::Right(s.to_string()),
        diff::Result::Both(s1, s2) => diff::Result::Both(s1.to_string(), s2.to_string()),
    }
}

pub fn diff_nonempty(diff: &[diff::Result<String>]) -> bool {
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

fn hunkify_diff(diff: &[diff::Result<String>], extra_lines: usize) -> HunkDiff {
    let mut hunks = vec![];

    let mut left_line_number: usize = 1;
    let mut right_line_number: usize = 1;

    let mut current_hunk = None;

    for (position, line) in diff.iter().enumerate() {
        match line {
            diff::Result::Left(_) | diff::Result::Right(_) => {
                // The central part of a hunk
                if current_hunk.is_none() {
                    current_hunk = Some((left_line_number, right_line_number, vec![]));
                }
                current_hunk.as_mut().unwrap().2.push(line.clone());
            }
            diff::Result::Both(_, _) => {
                if diff[position..=min(position + extra_lines, diff.len() - 1)]
                    .iter()
                    .any(is_different)
                {
                    // There's a hunk soon - but we might already be in a hunk
                    if current_hunk.is_none() {
                        current_hunk = Some((left_line_number, right_line_number, vec![]));
                    }
                    current_hunk.as_mut().unwrap().2.push(line.clone());
                } else if diff[position.saturating_sub(extra_lines)..position]
                    .iter()
                    .any(is_different)
                {
                    // We're just after a hunk
                    current_hunk.as_mut().unwrap().2.push(line.clone());
                } else if let Some(hunk) = current_hunk.take() {
                    // We're finished with the current hunk
                    hunks.push(hunk);
                }
            }
        }

        // Keep track of line numbers
        match line {
            diff::Result::Left(_) => {
                left_line_number += 1;
            }
            diff::Result::Right(_) => {
                right_line_number += 1;
            }
            diff::Result::Both(_, _) => {
                left_line_number += 1;
                right_line_number += 1;
            }
        }
    }

    // Last hunk - in case the last line is included in a hunk, it was never added
    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

fn is_different(diff: &diff::Result<String>) -> bool {
    !matches!(diff, diff::Result::Both(..))
}

fn print_hunk(mut left_line: usize, mut right_line: usize, hunk: Diff, max_digits: usize) {
    for line in hunk {
        match line {
            diff::Result::Left(l) => {
                println!(
                    " {:>width$} | {:>width$} | {}",
                    left_line.to_string().red(),
                    "",
                    l.red(),
                    width = max_digits
                );
                left_line += 1;
            }
            diff::Result::Both(l, _) => {
                println!(
                    " {:>width$} | {:>width$} | {}",
                    left_line.to_string().dark_grey(),
                    right_line.to_string().dark_grey(),
                    l,
                    width = max_digits
                );
                left_line += 1;
                right_line += 1;
            }
            diff::Result::Right(r) => {
                println!(
                    " {:>width$} | {:>width$} | {}",
                    "",
                    right_line.to_string().green(),
                    r.green(),
                    width = max_digits
                );
                right_line += 1;
            }
        }
    }
}

pub fn print_diff(diff: &[diff::Result<String>], extra_lines: usize) {
    let mut diff = hunkify_diff(diff, extra_lines);

    let last_hunk = diff.pop().expect("at least one hunk");
    let max_possible_line = max(last_hunk.0, last_hunk.1) + last_hunk.2.len();
    let max_possible_digits = max_possible_line.to_string().len(); // yes I could log10, whatever

    for hunk in diff {
        print_hunk(hunk.0, hunk.1, hunk.2, max_possible_digits);
        println!();
    }

    print_hunk(last_hunk.0, last_hunk.1, last_hunk.2, max_possible_digits);
}
