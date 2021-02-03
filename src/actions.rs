use std::path::Path;

use anyhow::{Context, Result};
use crossterm::style::Colorize;
use handlebars::Handlebars;

use crate::config::{SymbolicTarget, TemplateTarget, Variables};
use crate::difference;
use crate::filesystem::{Filesystem, SymlinkComparison, TemplateComparison};

// pub fn affect_cache(&self, cache: &mut crate::config::Cache) {
//     match self {
//         Action::DeleteSymlink { source, .. } => {
//             cache.symlinks.remove(source);
//         }
//         Action::DeleteTemplate { source, .. } => {
//             cache.templates.remove(source);
//         }
//         Action::CreateSymlink(s) => {
//             cache
//                 .symlinks
//                 .insert(s.source.clone(), s.target.target.clone());
//         }
//         Action::CreateTemplate(s) => {
//             cache
//                 .templates
//                 .insert(s.source.clone(), s.target.target.clone());
//         }
//         Action::UpdateSymlink(_) => {}
//         Action::UpdateTemplate(_) => {}
//     }
// }

// == DELETE ==

/// Returns true if symlink should be deleted from cache
pub fn delete_symlink(
    source: &Path,
    target: &Path,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    info!("{} symlink {:?} -> {:?}", "[-]".red(), source, target);

    let comparison = fs
        .compare_symlink(source, target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical | SymlinkComparison::OnlyTargetExists => {
            debug!("Performing deletion");
            perform_symlink_target_deletion(fs, target)
                .context("perform symlink target deletion")?;
            Ok(true)
        }
        SymlinkComparison::OnlySourceExists | SymlinkComparison::BothMissing => {
            warn!(
                "Deleting symlink {:?} -> {:?} but target doesn't exist. Removing from cache anyways.",
                source, target
            );
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!(
                "Deleting symlink {:?} -> {:?} but {}. Forcing.",
                source, target, comparison
            );
            perform_symlink_target_deletion(fs, target)
                .context("perform symlink target deletion")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!(
                "Deleting {:?} -> {:?} but {}. Skipping.",
                source, target, comparison
            );
            Ok(false)
        }
    }
}

fn perform_symlink_target_deletion(fs: &mut dyn Filesystem, target: &Path) -> Result<()> {
    fs.remove_file(target).context("remove symlink")?;
    fs.delete_parents(target)
        .context("delete parents of symlink")?;
    Ok(())
}

/// Returns true if template should be deleted from cache
pub fn delete_template(
    source: &Path,
    cache: &Path,
    target: &Path,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    info!("{} template {:?} -> {:?}", "[-]".red(), source, target);

    let comparison = fs
        .compare_template(target, cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::Identical => {
            debug!("Performing deletion");
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            perform_template_target_deletion(fs, target)
                .context("perform template target deletion")?;
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Deleting template {:?} -> {:?} but {}. Deleting cache anyways.",
                source, target, comparison
            );
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            Ok(true)
        }
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Deleting template {:?} -> {:?} but cache doesn't exist. Cache probably CORRUPTED.",
                source, target
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile if force => {
            warn!(
                "Deleting template {:?} -> {:?} but {}. Forcing.",
                source, target, comparison
            );
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            perform_template_target_deletion(fs, target)
                .context("perform template target deletion")?;
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile => {
            error!(
                "Deleting template {:?} -> {:?} but {}. Skipping.",
                source, target, comparison
            );
            Ok(false)
        }
    }
}

fn perform_cache_deletion(fs: &mut dyn Filesystem, cache: &Path) -> Result<()> {
    fs.remove_file(cache).context("delete template cache")?;
    fs.delete_parents(cache)
        .context("delete parent directory in cache")?;
    Ok(())
}

fn perform_template_target_deletion(fs: &mut dyn Filesystem, target: &Path) -> Result<()> {
    fs.remove_file(target).context("delete target file")?;
    fs.delete_parents(target)
        .context("delete parent directory in target location")?;
    Ok(())
}

// == CREATE ==

/// Returns true if symlink should be added to cache
pub fn create_symlink(
    source: &Path,
    target: &SymbolicTarget,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    info!(
        "{} symlink {:?} -> {:?}",
        "[+]".green(),
        source,
        target.target
    );

    let comparison = fs
        .compare_symlink(source, &target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlySourceExists => {
            debug!("Performing creation");
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            fs.make_symlink(&target.target, &source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Identical => {
            warn!("Creating symlink {:?} -> {:?} but target already exists and points at source. Adding to cache anyways", source, target.target);
            Ok(true)
        }
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!(
                "Creating symlink {:?} -> {:?} but {}. Skipping.",
                source, target.target, comparison
            );
            Ok(false)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!(
                "Creating symlink {:?} -> {:?} but {}. Forcing.",
                source, target.target, comparison
            );
            fs.remove_file(&target.target)
                .context("remove symlink target while forcing")?;
            fs.make_symlink(&target.target, &source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!(
                "Creating symlink {:?} -> {:?} but {}. Skipping.",
                source, target.target, comparison
            );
            Ok(false)
        }
    }
}

/// Returns true if the template should be added to cache
pub fn create_template(
    source: &Path,
    cache: &Path,
    target: &TemplateTarget,
    fs: &mut dyn Filesystem,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    force: bool,
) -> Result<bool> {
    info!(
        "{} template {:?} -> {:?}",
        "[+]".green(),
        source,
        target.target
    );

    let comparison = fs
        .compare_template(&target.target, &cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::BothMissing => {
            debug!("Performing creation");
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists | TemplateComparison::Identical => {
            warn!(
                "Creating template {:?} -> {:?} but cache file already exists. This is probably a result of an error in the last run.",
                source, target.target
            );
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::TargetNotRegularFile
        | TemplateComparison::Changed
        | TemplateComparison::OnlyTargetExists
            if force =>
        {
            warn!(
                "Creating template {:?} -> {:?} but target file already exists. Forcing.",
                source, target.target
            );
            fs.remove_file(&target.target)
                .context("remove existing file while forcing")?;
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::TargetNotRegularFile
        | TemplateComparison::Changed
        | TemplateComparison::OnlyTargetExists => {
            error!(
                "Creating template {:?} -> {:?} but target file already exists. Skipping.",
                source, target.target
            );
            Ok(false)
        }
    }
}

// == UPDATE ==

/// Returns true if the symlink wasn't skipped
pub fn update_symlink(
    source: &Path,
    target: &SymbolicTarget,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    debug!("Updating template {:?} -> {:?}...", source, target.target);

    let comparison = fs
        .compare_symlink(&source, &target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical => {
            debug!("Performing update");
            fs.set_owner(&target.target, &target.owner)
                .context("set target symlink owner")?;
            Ok(true)
        }
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!(
                "Updating template {:?} -> {:?} but source is missing. Skipping.",
                source, target.target
            );
            Ok(false)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!(
                "Updating template {:?} -> {:?} but {}. Forcing.",
                source, target.target, comparison
            );
            fs.remove_file(&target.target)
                .context("remove symlink target while forcing")?;
            fs.make_symlink(&target.target, &source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!(
                "Updating template {:?} -> {:?} but {}. Skipping.",
                source, target.target, comparison
            );
            Ok(false)
        }
        SymlinkComparison::OnlySourceExists => {
            warn!(
                "Updating template {:?} -> {:?} but {}. Creating it anyways.",
                source, target.target, comparison
            );
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            fs.make_symlink(&target.target, &source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
    }
}

/// Returns true if the template was not skipped
#[allow(clippy::too_many_arguments)]
pub fn update_template(
    source: &Path,
    cache: &Path,
    target: &TemplateTarget,
    fs: &mut dyn Filesystem,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
    force: bool,
    diff_context_lines: usize,
) -> Result<bool> {
    debug!("Updating template {:?} -> {:?}...", source, target.target);
    let comparison = fs
        .compare_template(&target.target, &cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::Identical => {
            debug!("Performing update");
            difference::print_template_diff(
                source,
                target,
                handlebars,
                variables,
                diff_context_lines,
            );
            fs.set_owner(&target.target, &target.owner)
                .context("set target file owner")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Updating template {:?} -> {:?} but target is missing. Creating it anyways.",
                source, target.target
            );
            fs.create_dir_all(
                &target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Updating template {:?} -> {:?} but cache is missing. Cache is CORRUPTED.",
                source, target.target
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile if force => {
            warn!(
                "Updating template {:?} -> {:?} but {}. Forcing.",
                source, target.target, comparison
            );
            difference::print_template_diff(
                source,
                target,
                handlebars,
                variables,
                diff_context_lines,
            );
            fs.remove_file(&target.target)
                .context("remove target while forcing")?;
            perform_template_deploy(source, &cache, target, fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::Changed | TemplateComparison::TargetNotRegularFile => {
            error!(
                "Updating template {:?} -> {:?} but {}. Skipping.",
                source, target.target, comparison
            );
            Ok(false)
        }
    }
}

pub(crate) fn perform_template_deploy(
    source: &Path,
    cache: &Path,
    target: &TemplateTarget,
    fs: &mut dyn Filesystem,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
) -> Result<()> {
    let file_contents = fs
        .read_to_string(&source)
        .context("read template source file")?;
    let file_contents = target.apply_actions(file_contents);
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    // Cache
    fs.create_dir_all(&cache.parent().context("get parent of cache file")?, &None)
        .context("create parent for cache file")?;
    fs.write(&cache, rendered)
        .context("write rendered template to cache")?;

    // Target
    fs.copy_file(&cache, &target.target, &target.owner)
        .context("copy template from cache to target")?;
    fs.copy_permissions(&source, &target.target, &target.owner)
        .context("copy permissions from source to target")?;

    Ok(())
}
