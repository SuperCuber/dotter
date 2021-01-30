use anyhow::{Context, Result};
use crossterm::style::Colorize;
use handlebars::Handlebars;

use std::fs;

use crate::args::Options;
use crate::config::Variables;
use crate::difference;
use crate::file_state::{
    FileState, SymlinkDescription, TemplateDescription,
};
use crate::filesystem::{self, Filesystem, SymlinkComparison, TemplateComparison};

pub fn plan_deploy(state: FileState) -> Vec<Action> {
    let mut actions = Vec::new();

    let FileState {
        desired_symlinks,
        desired_templates,
        existing_symlinks,
        existing_templates,
    } = state;

    for deleted_symlink in existing_symlinks.difference(&desired_symlinks) {
        actions.push(Action::DeleteSymlink(deleted_symlink.clone()));
    }

    for deleted_template in existing_templates.difference(&desired_templates) {
        actions.push(Action::DeleteTemplate(deleted_template.clone()));
    }

    for created_symlink in desired_symlinks.difference(&existing_symlinks) {
        actions.push(Action::CreateSymlink(created_symlink.clone()));
    }

    for created_template in desired_templates.difference(&existing_templates) {
        actions.push(Action::CreateTemplate(created_template.clone()));
    }

    for updated_symlink in desired_symlinks.intersection(&existing_symlinks) {
        actions.push(Action::UpdateSymlink(updated_symlink.clone()));
    }

    for updated_template in desired_templates.intersection(&existing_templates) {
        actions.push(Action::UpdateTemplate(updated_template.clone()));
    }

    actions
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    DeleteSymlink(SymlinkDescription),
    DeleteTemplate(TemplateDescription),
    CreateSymlink(SymlinkDescription),
    CreateTemplate(TemplateDescription),
    UpdateSymlink(SymlinkDescription),
    UpdateTemplate(TemplateDescription),
}

impl Action {
    /// Returns true if action was successfully performed (false if --force needed for it)
    pub fn run(&self, fs: &mut impl Filesystem, opt: &Options) -> Result<bool> {
        match self {
            Action::DeleteSymlink(s) => delete_symlink(&s, fs, opt.force),
            _ => todo!(),
        }
    }

    pub fn affect_cache(&self, cache: &mut crate::config::Cache) {
        match self {
            Action::DeleteSymlink(s) => {
                cache.symlinks.remove(&s.source);
            }
            _ => todo!(),
        }
    }
}

// == DELETE ==

/// Returns true if symlink should be deleted from cache
pub fn delete_symlink(
    symlink: &SymlinkDescription,
    fs: &mut impl Filesystem,
    force: bool,
) -> Result<bool> {
    info!("{} {}", "[-]".red(), symlink);

    let comparison = fs
        .compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical | SymlinkComparison::OnlyTargetExists => {
            debug!("Performing deletion");
            perform_symlink_target_deletion(fs, symlink)
                .context("perform symlink target deletion")?;
            Ok(true)
        }
        SymlinkComparison::OnlySourceExists | SymlinkComparison::BothMissing => {
            warn!(
                "Deleting {} but target doesn't exist. Removing from cache anyways.",
                symlink
            );
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!("Deleting {} but {}. Forcing.", symlink, comparison);
            perform_symlink_target_deletion(fs, symlink)
                .context("perform symlink target deletion")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!("Deleting {} but {}. Skipping.", symlink, comparison);
            Ok(false)
        }
    }
}

fn perform_symlink_target_deletion(
    fs: &mut impl Filesystem,
    symlink: &SymlinkDescription,
) -> Result<()> {
    fs.remove_file(&symlink.target.target)
        .context("remove symlink")?;
    fs.delete_parents(&symlink.target.target)
        .context("delete parents of symlink")?;
    Ok(())
}

/// Returns true if template should be deleted from cache
fn delete_template(
    act: bool,
    template: &TemplateDescription,
    force: bool,
    interactive: bool,
) -> Result<bool> {
    info!("{} {}", "[-]".red(), template);

    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::Identical => {
            debug!("Performing deletion");
            if act {
                perform_cache_deletion(template).context("perform cache deletion")?;
                perform_template_target_deletion(template, interactive)
                    .context("perform template target deletion")?;
            }
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Deleting {} but {}. Deleting cache anyways.",
                template, comparison
            );
            if act {
                perform_cache_deletion(template).context("perform cache deletion")?;
            }
            Ok(true)
        }
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Deleting {} but cache doesn't exist. Cache probably CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile if force => {
            warn!("Deleting {} but {}. Forcing.", template, comparison);
            // -f > -v
            perform_cache_deletion(template).context("perform cache deletion")?;
            perform_template_target_deletion(template, interactive)
                .context("perform template target deletion")?;
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile => {
            error!("Deleting {} but {}. Skipping.", template, comparison);
            Ok(false)
        }
    }
}

fn perform_cache_deletion(template: &TemplateDescription) -> Result<()> {
    fs::remove_file(&template.cache).context("delete template cache")?;
    filesystem::delete_parents(&template.cache, false)
        .context("delete parent directory in cache")?;
    Ok(())
}

fn perform_template_target_deletion(
    template: &TemplateDescription,
    interactive: bool,
) -> Result<()> {
    filesystem::remove_file(&template.target.target).context("delete target file")?;
    filesystem::delete_parents(&template.target.target, interactive)
        .context("delete parent directory in target location")?;
    Ok(())
}

// == CREATE ==

/// Returns true if symlink should be added to cache
fn create_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    info!("{} {}", "[+]".green(), symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlySourceExists => {
            debug!("Performing creation");
            if act {
                filesystem::create_dir_all(
                    &symlink
                        .target
                        .target
                        .parent()
                        .context("get parent of target file")?,
                    &symlink.target.owner,
                )
                .context("create parent for target file")?;
                filesystem::make_symlink(
                    &symlink.target.target,
                    &symlink.source,
                    &symlink.target.owner,
                )
                .context("create target symlink")?;
            }
            Ok(true)
        }
        SymlinkComparison::Identical => {
            warn!("Creating {} but target already exists and points at source. Adding to cache anyways", symlink);
            Ok(true)
        }
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!("Creating {} but {}. Skipping.", symlink, comparison);
            Ok(false)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!("Creating {} but {}. Forcing.", symlink, comparison);
            filesystem::remove_file(&symlink.target.target)
                .context("remove symlink target while forcing")?;
            // -f > -v
            filesystem::make_symlink(
                &symlink.target.target,
                &symlink.source,
                &symlink.target.owner,
            )
            .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!("Creating {} but {}. Skipping.", symlink, comparison);
            Ok(false)
        }
    }
}

// Returns true if the template should be added to cache
fn create_template(
    act: bool,
    template: &TemplateDescription,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    force: bool,
) -> Result<bool> {
    info!("{} {}", "[+]".green(), template);

    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::BothMissing => {
            debug!("Performing creation");
            if act {
                filesystem::create_dir_all(
                    &template
                        .target
                        .target
                        .parent()
                        .context("get parent of target file")?,
                    &template.target.owner,
                )
                .context("create parent for target file")?;
                perform_template_deploy(template, handlebars, variables)
                    .context("perform template cache")?;
            }
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists | TemplateComparison::Identical => {
            warn!(
                "Creating {} but cache file already exists. This is probably a result of an error in the last run.",
                template
            );
            if act {
                filesystem::create_dir_all(
                    &template
                        .target
                        .target
                        .parent()
                        .context("get parent of target file")?,
                    &template.target.owner,
                )
                .context("create parent for target file")?;
                perform_template_deploy(template, handlebars, variables)
                    .context("perform template cache")?;
            }
            Ok(true)
        }
        TemplateComparison::TargetNotRegularFile
        | TemplateComparison::Changed
        | TemplateComparison::OnlyTargetExists
            if force =>
        {
            warn!(
                "Creating {} but target file already exists. Forcing.",
                template
            );
            filesystem::remove_file(&template.target.target)
                .context("remove existing file while forcing")?;
            // -f > -v
            filesystem::create_dir_all(
                &template
                    .target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &template.target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(template, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::TargetNotRegularFile
        | TemplateComparison::Changed
        | TemplateComparison::OnlyTargetExists => {
            error!(
                "Creating {} but target file already exists. Skipping.",
                template
            );
            Ok(false)
        }
    }
}

// == UPDATE ==

/// Returns true if the symlink wasn't skipped
fn update_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    debug!("Updating {}...", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical => {
            debug!("Performing update");
            if act {
                filesystem::set_owner(&symlink.target.target, &symlink.target.owner)
                    .context("set target symlink owner")?;
            }
            Ok(true)
        }
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!("Updating {} but source is missing. Skipping.", symlink);
            Ok(false)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!("Updating {} but {}. Forcing.", symlink, comparison);
            filesystem::remove_file(&symlink.target.target)
                .context("remove symlink target while forcing")?;
            // -f > -v
            filesystem::make_symlink(
                &symlink.target.target,
                &symlink.source,
                &symlink.target.owner,
            )
            .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!("Updating {} but {}. Skipping.", symlink, comparison);
            Ok(false)
        }
        SymlinkComparison::OnlySourceExists => {
            warn!(
                "Updating {} but {}. Creating it anyways.",
                symlink, comparison
            );
            if act {
                filesystem::create_dir_all(
                    &symlink
                        .target
                        .target
                        .parent()
                        .context("get parent of target file")?,
                    &symlink.target.owner,
                )
                .context("create parent for target file")?;
                filesystem::make_symlink(
                    &symlink.target.target,
                    &symlink.source,
                    &symlink.target.owner,
                )
                .context("create target symlink")?;
            }
            Ok(true)
        }
    }
}

/// Returns true if the template was not skipped
fn update_template(
    act: bool,
    template: &TemplateDescription,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    force: bool,
    diff_context_lines: usize,
) -> Result<bool> {
    debug!("Updating {}...", template);
    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::Identical => {
            debug!("Performing update");
            difference::print_template_diff(template, handlebars, variables, diff_context_lines);
            if act {
                filesystem::set_owner(&template.target.target, &template.target.owner)
                    .context("set target file owner")?;
                perform_template_deploy(template, handlebars, variables)
                    .context("perform template cache")?;
            }
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Updating {} but target is missing. Creating it anyways.",
                template
            );
            filesystem::create_dir_all(
                &template
                    .target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &template.target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(template, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Updating {} but cache is missing. Cache is CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile if force => {
            warn!("Updating {} but {}. Forcing.", template, comparison);
            difference::print_template_diff(template, handlebars, variables, diff_context_lines);
            // -f > -v
            filesystem::remove_file(&template.target.target)
                .context("remove target while forcing")?;
            perform_template_deploy(template, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile => {
            error!("Updating {} but {}. Skipping.", template, comparison);
            Ok(false)
        }
    }
}

pub(crate) fn perform_template_deploy(
    template: &TemplateDescription,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
) -> Result<()> {
    let file_contents =
        fs::read_to_string(&template.source).context("read template source file")?;
    let file_contents = template.apply_actions(file_contents);
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    // Cache
    fs::create_dir_all(
        &template
            .cache
            .parent()
            .context("get parent of cache file")?,
    )
    .context("create parent for cache file")?;
    fs::write(&template.cache, rendered).context("write rendered template to cache")?;

    // Target
    filesystem::copy_file(
        &template.cache,
        &template.target.target,
        &template.target.owner,
    )
    .context("copy template from cache to target")?;
    filesystem::copy_permissions(
        &template.source,
        &template.target.target,
        &template.target.owner,
    )
    .context("copy permissions from source to target")?;

    Ok(())
}
#[cfg(test)]
mod test {
    use crate::config::{SymbolicTarget, TemplateTarget};

    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn initial_deploy() {
        let a = SymlinkDescription {
            source: "a_in".into(),
            target: SymbolicTarget {
                target: "a_out".into(),
                owner: None,
            },
        };
        let b = TemplateDescription {
            source: "b_in".into(),
            target: TemplateTarget {
                target: "b_out".into(),
                owner: None,
                append: None,
                prepend: None,
            },
            cache: "cache/b_cache".into(),
        };
        let file_state = FileState {
            desired_symlinks: maplit::btreeset! {
                a.clone()
            },
            desired_templates: maplit::btreeset! {
                b.clone()
            },
            existing_symlinks: BTreeSet::new(),
            existing_templates: BTreeSet::new(),
        };

        let actions = plan_deploy(file_state);
        assert_eq!(
            actions,
            [Action::CreateSymlink(a), Action::CreateTemplate(b)]
        );

        let mut fs = crate::filesystem::MockFilesystem::new();
        fs.expect_remove_file().times(1).returning(|_p| Ok(()));

        actions[0].run(&mut fs, &Options::default()).unwrap();
    }
}
