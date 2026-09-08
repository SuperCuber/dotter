use std::path::Path;

use anyhow::{Context, Result};

use crossterm::style::Stylize;
use handlebars::Handlebars;

use crate::config::{CopyTarget, SymbolicTarget, TemplateTarget, UnixUser, Variables};
use crate::difference::{
    self, Diff, diff_nonempty, generate_copy_diff, generate_template_diff, print_diff,
};
use crate::filesystem::{CachedFileComparison, Filesystem, SymlinkComparison};

#[cfg_attr(test, mockall::automock)]
pub trait ActionRunner {
    fn delete_symlink(&mut self, source: &Path, target: &Path) -> Result<bool>;
    fn delete_template(&mut self, source: &Path, cache: &Path, target: &Path) -> Result<bool>;
    fn delete_copy(&mut self, source: &Path, cache: &Path, target: &Path) -> Result<bool>;
    fn create_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool>;
    fn create_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool>;
    fn create_copy(&mut self, source: &Path, cache: &Path, target: &CopyTarget) -> Result<bool>;
    fn update_symlink(&mut self, source: &Path, target: &SymbolicTarget) -> Result<bool>;
    fn update_template(
        &mut self,
        source: &Path,
        cache: &Path,
        target: &TemplateTarget,
    ) -> Result<bool>;
    fn update_copy(&mut self, source: &Path, cache: &Path, target: &CopyTarget) -> Result<bool>;
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
    fn delete_copy(&mut self, source: &Path, cache: &Path, target: &Path) -> Result<bool> {
        delete_copy(source, cache, target, self.fs, self.force)
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
    fn create_copy(&mut self, source: &Path, cache: &Path, target: &CopyTarget) -> Result<bool> {
        create_copy(source, cache, target, self.fs, self.force)
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
    fn update_copy(&mut self, source: &Path, cache: &Path, target: &CopyTarget) -> Result<bool> {
        update_copy(
            source,
            cache,
            target,
            self.fs,
            self.force,
            self.diff_context_lines,
        )
    }
}

// == CACHED TARGETS ==

/// A target that is deployed through the cache directory - a template or a copy.
enum CachedTarget<'a> {
    Template {
        target: &'a TemplateTarget,
        handlebars: &'a Handlebars<'a>,
        variables: &'a Variables,
    },
    Copy(&'a CopyTarget),
}

/// How a target's contents differ from what would be deployed to it right now.
enum TargetModification {
    Diff(Diff),
    /// The contents differ, but at least one of the two sides isn't valid UTF-8, so only
    /// their inequality can be reported and not the difference itself.
    Binary,
}

impl CachedTarget<'_> {
    fn kind(&self) -> &'static str {
        match self {
            CachedTarget::Template { .. } => "template",
            CachedTarget::Copy(_) => "copy",
        }
    }

    fn target(&self) -> &Path {
        match self {
            CachedTarget::Template { target, .. } => &target.target,
            CachedTarget::Copy(target) => &target.target,
        }
    }

    fn owner(&self) -> &Option<UnixUser> {
        match self {
            CachedTarget::Template { target, .. } => &target.owner,
            CachedTarget::Copy(target) => &target.owner,
        }
    }

    fn print_diff(&self, source: &Path, diff_context_lines: usize) {
        match *self {
            CachedTarget::Template {
                target,
                handlebars,
                variables,
            } => difference::print_template_diff(
                source,
                target,
                handlebars,
                variables,
                diff_context_lines,
            ),
            CachedTarget::Copy(target) => {
                difference::print_copy_diff(source, target, diff_context_lines)
            }
        }
    }

    fn deploy(&self, source: &Path, cache: &Path, fs: &mut dyn Filesystem) -> Result<()> {
        match *self {
            CachedTarget::Template {
                target,
                handlebars,
                variables,
            } => perform_template_deploy(source, cache, Some(target), fs, handlebars, variables)
                .context("perform template cache"),
            CachedTarget::Copy(target) => {
                perform_copy_deploy(source, cache, target, fs).context("perform copy")
            }
        }
    }

    fn create_parent(&self, fs: &mut dyn Filesystem) -> Result<()> {
        fs.create_dir_all(
            self.target()
                .parent()
                .context("get parent of target file")?,
            self.owner(),
        )
        .context("create parent for target file")
    }

    /// Compares the target's contents against what would be deployed to it now. Returns `None`
    /// when they are equal - the target was modified, but into the contents it is about to
    /// receive anyways, so there is nothing of the user's to lose by overwriting it.
    fn modification(&self, source: &Path) -> Result<Option<TargetModification>> {
        match *self {
            CachedTarget::Template {
                target,
                handlebars,
                variables,
            } => {
                let diff = generate_template_diff(source, target, handlebars, variables, false)?;
                Ok(diff_nonempty(&diff).then_some(TargetModification::Diff(diff)))
            }
            CachedTarget::Copy(target) => {
                Ok(match generate_copy_diff(source, &target.target, false)? {
                    Some(diff) if !diff_nonempty(&diff) => None,
                    Some(diff) => Some(TargetModification::Diff(diff)),
                    None => Some(TargetModification::Binary),
                })
            }
        }
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
    delete_cached_file("template", source, cache, target, fs, force)
}

/// Returns true if copy should be deleted from cache
pub fn delete_copy(
    source: &Path,
    cache: &Path,
    target: &Path,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    delete_cached_file("copy", source, cache, target, fs, force)
}

/// Deletes a file that was deployed through the cache - a template or a copy.
/// Returns true if it should be deleted from cache
fn delete_cached_file(
    kind: &str,
    source: &Path,
    cache: &Path,
    target: &Path,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    info!("{} {} {:?} -> {:?}", "[-]".red(), kind, source, target);

    let comparison = fs
        .compare_cached_file(target, cache)
        .context("detect deployed file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        CachedFileComparison::Identical => {
            debug!("Performing deletion");
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            perform_cached_target_deletion(fs, target).context("perform target deletion")?;
            Ok(true)
        }
        CachedFileComparison::OnlyCacheExists => {
            warn!(
                "Deleting {} {:?} -> {:?} but {}. Deleting cache anyways.",
                kind, source, target, comparison
            );
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            Ok(true)
        }
        CachedFileComparison::OnlyTargetExists | CachedFileComparison::BothMissing => {
            error!(
                "Deleting {} {:?} -> {:?} but cache doesn't exist. Cache probably CORRUPTED.",
                kind, source, target
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        CachedFileComparison::Changed | CachedFileComparison::TargetNotRegularFile if force => {
            warn!(
                "Deleting {} {:?} -> {:?} but {}. Forcing.",
                kind, source, target, comparison
            );
            perform_cache_deletion(fs, cache).context("perform cache deletion")?;
            perform_cached_target_deletion(fs, target).context("perform target deletion")?;
            Ok(true)
        }
        CachedFileComparison::Changed | CachedFileComparison::TargetNotRegularFile => {
            error!(
                "Deleting {} {:?} -> {:?} but {}. Skipping.",
                kind, source, target, comparison
            );
            Ok(false)
        }
    }
}

fn perform_cache_deletion(fs: &mut dyn Filesystem, cache: &Path) -> Result<()> {
    fs.remove_file(cache).context("delete cache file")?;
    fs.delete_parents(cache, true)
        .context("delete parent directory in cache")?;
    Ok(())
}

fn perform_cached_target_deletion(fs: &mut dyn Filesystem, target: &Path) -> Result<()> {
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
    create_cached_file(
        source,
        cache,
        &CachedTarget::Template {
            target,
            handlebars,
            variables,
        },
        fs,
        force,
    )
}

/// Returns true if the copy should be added to cache
pub fn create_copy(
    source: &Path,
    cache: &Path,
    target: &CopyTarget,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    create_cached_file(source, cache, &CachedTarget::Copy(target), fs, force)
}

/// Creates a file that is deployed through the cache - a template or a copy.
/// Returns true if it should be added to cache
fn create_cached_file(
    source: &Path,
    cache: &Path,
    target: &CachedTarget<'_>,
    fs: &mut dyn Filesystem,
    force: bool,
) -> Result<bool> {
    let kind = target.kind();
    let target_path = target.target();
    info!(
        "{} {} {:?} -> {:?}",
        "[+]".green(),
        kind,
        source,
        target_path
    );

    let comparison = fs
        .compare_cached_file(target_path, cache)
        .context("detect deployed file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        CachedFileComparison::BothMissing => {
            debug!("Performing creation");
            target.create_parent(fs)?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::OnlyCacheExists | CachedFileComparison::Identical => {
            warn!(
                "Creating {} {:?} -> {:?} but cache file already exists. This is probably a result of an error in the last run.",
                kind, source, target_path
            );
            target.create_parent(fs)?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::TargetNotRegularFile
        | CachedFileComparison::Changed
        | CachedFileComparison::OnlyTargetExists
            if force =>
        {
            warn!(
                "Creating {} {:?} -> {:?} but target file already exists. Forcing.",
                kind, source, target_path
            );
            fs.remove_file(target_path)
                .context("remove existing file while forcing")?;
            target.create_parent(fs)?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::TargetNotRegularFile
        | CachedFileComparison::Changed
        | CachedFileComparison::OnlyTargetExists => {
            error!(
                "Creating {} {:?} -> {:?} but target file already exists. Skipping.",
                kind, source, target_path
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
    update_cached_file(
        source,
        cache,
        &CachedTarget::Template {
            target,
            handlebars,
            variables,
        },
        fs,
        force,
        diff_context_lines,
    )
}

/// Returns true if the copy was not skipped
pub fn update_copy(
    source: &Path,
    cache: &Path,
    target: &CopyTarget,
    fs: &mut dyn Filesystem,
    force: bool,
    diff_context_lines: usize,
) -> Result<bool> {
    update_cached_file(
        source,
        cache,
        &CachedTarget::Copy(target),
        fs,
        force,
        diff_context_lines,
    )
}

/// Updates a file that is deployed through the cache - a template or a copy.
/// Returns true if it was not skipped
fn update_cached_file(
    source: &Path,
    cache: &Path,
    target: &CachedTarget<'_>,
    fs: &mut dyn Filesystem,
    force: bool,
    diff_context_lines: usize,
) -> Result<bool> {
    let kind = target.kind();
    let target_path = target.target();
    debug!("Updating {} {:?} -> {:?}...", kind, source, target_path);

    let comparison = fs
        .compare_cached_file(target_path, cache)
        .context("detect deployed file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        CachedFileComparison::Identical => {
            debug!("Performing update");
            target.print_diff(source, diff_context_lines);
            fs.set_owner(target_path, target.owner())
                .context("set target file owner")?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::OnlyCacheExists => {
            warn!(
                "Updating {} {:?} -> {:?} but target is missing. Creating it anyways.",
                kind, source, target_path
            );
            target.create_parent(fs)?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::OnlyTargetExists | CachedFileComparison::BothMissing => {
            error!(
                "Updating {} {:?} -> {:?} but cache is missing. Cache is CORRUPTED.",
                kind, source, target_path
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(true)
        }
        CachedFileComparison::Changed | CachedFileComparison::TargetNotRegularFile if force => {
            warn!(
                "Updating {} {:?} -> {:?} but {}. Forcing.",
                kind, source, target_path, comparison
            );
            target.print_diff(source, diff_context_lines);
            fs.remove_file(target_path)
                .context("remove target while forcing")?;
            target.deploy(source, cache, fs)?;
            Ok(true)
        }
        CachedFileComparison::Changed => {
            match target
                .modification(source)
                .context("diff source and target")?
            {
                None => {
                    target.deploy(source, cache, fs)?;
                    Ok(true)
                }
                Some(modification) => {
                    error!(
                        "Updating {} {:?} -> {:?} but {}. Skipping",
                        kind, source, target_path, comparison
                    );
                    if log_enabled!(log::Level::Info) {
                        info!("Refusing because of the following changes in target location: ");
                        match modification {
                            TargetModification::Diff(diff) => print_diff(&diff, diff_context_lines),
                            TargetModification::Binary => {
                                info!("Target's contents are binary and differ from the source")
                            }
                        }
                    }
                    Ok(false)
                }
            }
        }
        CachedFileComparison::TargetNotRegularFile => {
            error!(
                "Updating {} {:?} -> {:?} but {}. Skipping.",
                kind, source, target_path, comparison
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

fn perform_copy_deploy(
    source: &Path,
    cache: &Path,
    target: &CopyTarget,
    fs: &mut dyn Filesystem,
) -> Result<()> {
    // Cache
    fs.create_dir_all(cache.parent().context("get parent of cache file")?, &None)
        .context("create parent for cache file")?;
    fs.copy_file(source, cache, &None)
        .context("copy source file to cache")?;

    // Target
    fs.copy_file(cache, &target.target, &target.owner)
        .context("copy file from cache to target")?;
    fs.copy_permissions(source, &target.target, &target.owner)
        .context("copy permissions from source to target")?;

    Ok(())
}
