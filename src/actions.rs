use std::path::Path;

use anyhow::{Context, Result};

use crossterm::style::Stylize;
use handlebars::Handlebars;

use crate::config::{SymbolicTarget, TemplateTarget, Variables};
use crate::difference::{self, diff_nonempty, generate_template_diff, print_diff};
use crate::filesystem::{Filesystem, SymlinkComparison, TemplateComparison};

#[cfg_attr(test, mockall::automock)]
pub trait ActionRunner {
    fn delete_symlink(&mut self, source: &Path, target: &Path) -> Result<bool>;
    fn delete_template(&mut self, source: &Path, cache: &Path, target: &Path) -> Result<bool>;
    fn create_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool>;
    fn create_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool>;
    fn update_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool>;
    fn update_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool>;
}

pub struct RealActionRunner<'a> {
    fs: &'a mut dyn Filesystem,
    handlebars: &'a Handlebars<'a>,
    variables: &'a Variables,
    force: bool,
    diff_context_lines: usize,
}

impl<'a> RealActionRunner<'a> {
    pub fn new(
        fs: &'a mut dyn Filesystem,
        handlebars: &'a Handlebars<'_>,
        variables: &'a Variables,
        force: bool,
        diff_context_lines: usize,
    ) -> RealActionRunner<'a> {
        RealActionRunner {
            fs,
            handlebars,
            variables,
            force,
            diff_context_lines,
        }
    }
}

impl ActionRunner for RealActionRunner<'_> {
    fn delete_symlink(&mut self, source: &Path, target: &Path) -> Result<bool> {
        delete_symlink(source, target, self.fs, self.force)
    }
    fn delete_template(&mut self, source: &Path, cache: &Path, target: &Path) -> Result<bool> {
        delete_template(source, cache, target, self.fs, self.force)
    }
    fn create_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool> {
        create_symlink(source, target, self.fs, self.force)
    }
    fn create_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool> {
        create_template(
            source,
            cache,
            target,
            self.fs,
            self.handlebars,
            self.variables,
            self.force,
        )
    }
    fn update_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool> {
        update_symlink(source, target, self.fs, self.force)
    }
    fn update_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool> {
        update_template(
            source,
            cache,
            target,
            self.fs,
            self.handlebars,
            self.variables,
            self.force,
            self.diff_context_lines,
        )
    }
}

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
    fs.delete_parents(target, false)
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
    fs.delete_parents(cache, true)
        .context("delete parent directory in cache")?;
    Ok(())
}

fn perform_template_target_deletion(fs: &mut dyn Filesystem, target: &Path) -> Result<()> {
    fs.remove_file(target).context("delete target file")?;
    fs.delete_parents(target, false)
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
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            fs.make_symlink(&target.target, source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Identical => {
            warn!(
                "Creating symlink {:?} -> {:?} but target already exists and points at source. Adding to cache anyways",
                source, target.target
            );
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
            fs.make_symlink(&target.target, source, &target.owner)
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
        .compare_template(&target.target, cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::BothMissing => {
            debug!("Performing creation");
            fs.create_dir_all(
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists | TemplateComparison::Identical => {
            warn!(
                "Creating template {:?} -> {:?} but cache file already exists. This is probably a result of an error in the last run.",
                source, target.target
            );
            fs.create_dir_all(
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
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
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
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
    debug!("Updating symlink {:?} -> {:?}...", source, target.target);

    let comparison = fs
        .compare_symlink(source, &target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical => {
            debug!("Performing update");
            Ok(true)
        }
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!(
                "Updating symlink {:?} -> {:?} but source is missing. Skipping.",
                source, target.target
            );
            Ok(false)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if force => {
            warn!(
                "Updating symlink {:?} -> {:?} but {}. Forcing.",
                source, target.target, comparison
            );
            fs.remove_file(&target.target)
                .context("remove symlink target while forcing")?;
            fs.make_symlink(&target.target, source, &target.owner)
                .context("create target symlink")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!(
                "Updating symlink {:?} -> {:?} but {}. Skipping.",
                source, target.target, comparison
            );
            Ok(false)
        }
        SymlinkComparison::OnlySourceExists => {
            warn!(
                "Updating symlink {:?} -> {:?} but {}. Creating it anyways.",
                source, target.target, comparison
            );
            fs.create_dir_all(
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            fs.make_symlink(&target.target, source, &target.owner)
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
        .compare_template(&target.target, cache)
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
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Updating template {:?} -> {:?} but target is missing. Creating it anyways.",
                source, target.target
            );
            fs.create_dir_all(
                target
                    .target
                    .parent()
                    .context("get parent of target file")?,
                &target.owner,
            )
            .context("create parent for target file")?;
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
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
            perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
                .context("perform template cache")?;
            Ok(true)
        }
        TemplateComparison::Changed => {
            // At this point, we're not sure if there's a difference between the rendered source
            // and target, only that the target has been modified in some way.
            let diff = generate_template_diff(source, target, handlebars, variables, false)
                .context("diff source and target")?;
            if diff_nonempty(&diff) {
                error!(
                    "Updating template {:?} -> {:?} but {}. Skipping",
                    source, target.target, comparison
                );
                if log_enabled!(log::Level::Info) {
                    info!("Refusing because of the following changes in target location: ");
                    print_diff(&diff, diff_context_lines);
                }
                Ok(false)
            } else {
                perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
                    .context("perform template cache")?;
                Ok(true)
            }
        }

        TemplateComparison::TargetNotRegularFile => {
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
    target: Option<&TemplateTarget>,
    fs: &mut dyn Filesystem,
    handlebars: &Handlebars<'_>,
    variables: &Variables,
) -> Result<()> {
    let file_contents = fs
        .read_to_string(source)
        .context("read template source file")?;
    let file_contents = match target {
        Some(t) => t.apply_actions(file_contents),
        None => file_contents,
    };
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    // Cache
    fs.create_dir_all(cache.parent().context("get parent of cache file")?, &None)
        .context("create parent for cache file")?;
    fs.write(cache, rendered)
        .context("write rendered template to cache")?;

    // Target
    if let Some(target) = target {
        fs.copy_file(cache, &target.target, &target.owner)
            .context("copy template from cache to target")?;
        fs.copy_permissions(source, &target.target, &target.owner)
            .context("copy permissions from source to target")?;
    }

    Ok(())
}
