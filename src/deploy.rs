use anyhow::{Context, Result};
use crossterm::style::Colorize;

use handlebars::Handlebars;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::display_error;
use args::Options;
use config::{self, Variables};
use difference;
use file_state::{FileState, SymlinkDescription, TemplateDescription};
use filesystem::{self, SymlinkComparison, TemplateComparison};
use handlebars_helpers;

pub fn undeploy(opt: Options) -> Result<()> {
    let cache = config::load_cache(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let config::Cache {
        symlinks: existing_symlinks,
        templates: existing_templates,
    } = cache;

    // Used just to transform them into Description structs
    let state = FileState::new(
        BTreeMap::default(),
        BTreeMap::default(),
        existing_symlinks.clone(),
        existing_templates.clone(),
        opt.cache_directory,
    );
    trace!("File state: {:#?}", state);

    let (deleted_symlinks, deleted_templates) = state.deleted_files();

    let mut actual_symlinks = existing_symlinks;
    let mut actual_templates = existing_templates;
    let mut suggest_force = false;

    for symlink in deleted_symlinks {
        match delete_symlink(opt.act, &symlink, opt.force, opt.interactive) {
            Ok(true) => {
                actual_symlinks.remove(&symlink.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete {}", symlink))),
        }
    }

    for template in deleted_templates {
        match delete_template(opt.act, &template, opt.force, opt.interactive) {
            Ok(true) => {
                actual_templates.remove(&template.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete {}", template))),
        }
    }

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
    }

    if opt.act {
        // Should be empty if everything went well, but if some things were skipped this contains
        // them.
        config::save_cache(
            &opt.cache_file,
            config::Cache {
                symlinks: actual_symlinks,
                templates: actual_templates,
            },
        )?;
    }

    Ok(())
}

pub fn file_state_from_configuration(
    config: &config::Configuration,
    cache: &config::Cache,
    cache_directory: &Path,
) -> Result<FileState> {
    // On Windows, you need developer mode to create symlinks.
    let symlinks_enabled = if filesystem::symlinks_enabled(&PathBuf::from("DOTTER_SYMLINK_TEST"))
        .context("check whether symlinks are enabled")?
    {
        true
    } else {
        warn!(
            "No permission to create symbolic links.\n
On Windows, in order to create symbolic links you need to enable Developer Mode.\n
Proceeding by copying instead of symlinking."
        );
        false
    };

    let mut desired_symlinks = BTreeMap::new();
    let mut desired_templates = BTreeMap::new();

    for (source, target) in config.files.clone() {
        match target {
            config::FileTarget::Automatic(target) => {
                if symlinks_enabled
                    && !is_template(&source)
                        .context(format!("check whether {:?} is a template", source))?
                {
                    desired_symlinks.insert(
                        source,
                        config::SymbolicTarget {
                            target,
                            owner: None,
                        },
                    );
                } else {
                    desired_templates.insert(
                        source,
                        config::TemplateTarget {
                            target,
                            owner: None,
                            append: None,
                            prepend: None,
                        },
                    );
                }
            }
            config::FileTarget::Symbolic(target) => {
                if symlinks_enabled {
                    desired_symlinks.insert(source, target);
                } else {
                    desired_templates.insert(
                        source,
                        config::TemplateTarget {
                            target: target.target,
                            owner: target.owner,
                            append: None,
                            prepend: None,
                        },
                    );
                }
            }
            config::FileTarget::ComplexTemplate(target) => {
                desired_templates.insert(source, target);
            }
        }
    }

    trace!("Desired symlinks: {:#?}", desired_symlinks);
    trace!("Desired templates: {:#?}", desired_templates);

    let state = FileState::new(
        desired_symlinks,
        desired_templates,
        cache.symlinks.clone(),
        cache.templates.clone(),
        cache_directory,
    );

    Ok(state)
}

/// Returns true if an error was printed
pub fn deploy(opt: &Options) -> Result<bool> {
    let mut patch = None;
    if opt.patch {
        debug!("Reading manual patch from stdin...");
        let mut patch_str = String::new();
        io::stdin()
            .read_to_string(&mut patch_str)
            .context("read patch from stdin")?;
        patch = Some(toml::from_str(&patch_str).context("parse patch into package")?);
    }
    trace!("Manual patch: {:#?}", patch);

    let config = config::load_configuration(&opt.local_config, &opt.global_config, patch)
        .context("get a configuration")?;

    let cache = if let Some(cache) = config::load_cache(&opt.cache_file)? {
        cache
    } else {
        warn!("Cache file not found. Assuming cache is empty.");
        config::Cache::default()
    };

    let state = file_state_from_configuration(&config, &cache, &opt.cache_directory)
        .context("get file state")?;
    trace!("File state: {:#?}", state);

    let config::Configuration {
        files,
        mut variables,
        helpers,
        packages,
    } = config;

    let config::Cache {
        symlinks: mut actual_symlinks,
        templates: mut actual_templates,
    } = cache;

    let mut suggest_force = false;
    let mut error_occurred = false;

    let (deleted_symlinks, deleted_templates) = state.deleted_files();
    trace!("Deleted symlinks: {:#?}", deleted_symlinks);
    trace!("Deleted templates: {:#?}", deleted_templates);
    for deleted_symlink in deleted_symlinks {
        match delete_symlink(opt.act, &deleted_symlink, opt.force, opt.interactive) {
            Ok(true) => {
                actual_symlinks.remove(&deleted_symlink.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("delete {}", deleted_symlink)));
                error_occurred = true;
            }
        }
    }
    for deleted_template in deleted_templates {
        match delete_template(opt.act, &deleted_template, opt.force, opt.interactive) {
            Ok(true) => {
                actual_templates.remove(&deleted_template.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("delete {}", deleted_template)));
                error_occurred = true;
            }
        }
    }

    // Prepare handlebars instance
    debug!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, &helpers);
    handlebars_helpers::add_dotter_variable(&mut variables, &files, &packages);
    trace!("Handlebars instance: {:#?}", handlebars);

    let (new_symlinks, new_templates) = state.new_files();
    trace!("New symlinks: {:#?}", new_symlinks);
    trace!("New templates: {:#?}", new_templates);
    for new_symlink in new_symlinks {
        match create_symlink(opt.act, &new_symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.insert(new_symlink.source, new_symlink.target.target);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("create {}", new_symlink)));
                error_occurred = true;
            }
        }
    }
    for new_template in new_templates {
        match create_template(opt.act, &new_template, &handlebars, &variables, opt.force) {
            Ok(true) => {
                actual_templates.insert(new_template.source, new_template.target.target);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("create {}", new_template)));
                error_occurred = true;
            }
        }
    }

    let (old_symlinks, old_templates) = state.old_files();
    trace!("Old symlinks: {:#?}", old_symlinks);
    trace!("Old templates: {:#?}", old_templates);
    for old_symlink in old_symlinks {
        match update_symlink(opt.act, &old_symlink, opt.force) {
            Ok(true) => {}
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("update {}", old_symlink)));
                error_occurred = true;
            }
        }
    }
    for old_template in old_templates {
        match update_template(
            opt.act,
            &old_template,
            &handlebars,
            &variables,
            opt.force,
            opt.diff_context_lines,
        ) {
            Ok(true) => {}
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("update {}", old_template)));
                error_occurred = true;
            }
        }
    }

    trace!("Actual symlinks: {:#?}", actual_symlinks);
    trace!("Actual templates: {:#?}", actual_templates);

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
        error_occurred = true;
    }

    if opt.act {
        config::save_cache(
            &opt.cache_file,
            config::Cache {
                symlinks: actual_symlinks,
                templates: actual_templates,
            },
        )?;
    }

    Ok(error_occurred)
}

// == DELETE ==

/// Returns true if symlink should be deleted from cache
fn delete_symlink(
    act: bool,
    symlink: &SymlinkDescription,
    force: bool,
    interactive: bool,
) -> Result<bool> {
    info!("{} {}", "[-]".red(), symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlySourceExists | SymlinkComparison::BothMissing => {
            warn!(
                "Deleting {} but target doesn't exist. Removing from cache anyways.",
                symlink
            );
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            if force {
                warn!(
                    "Deleting {} but target wasn't what was expected. Forcing.",
                    symlink
                );
                debug!("Performing deletion");
                if act {
                    perform_symlink_target_deletion(symlink, interactive)
                        .context("perform symlink target deletion")?;
                }
                Ok(true)
            } else {
                if comparison == SymlinkComparison::Changed {
                    error!(
                        "Deleting {} but target doesn't point at source file. Skipping...",
                        symlink
                    );
                } else {
                    // SymlinkComparison::TargetNotSymlink
                    error!(
                        "Deleting {} but target isn't a symlink. Skipping...",
                        symlink
                    );
                }
                Ok(false)
            }
        }
        SymlinkComparison::Identical | SymlinkComparison::OnlyTargetExists => {
            debug!("Performing deletion");
            if act {
                perform_symlink_target_deletion(symlink, interactive)
                    .context("perform symlink target deletion")?;
            }
            Ok(true)
        }
    }
}

fn perform_symlink_target_deletion(symlink: &SymlinkDescription, interactive: bool) -> Result<()> {
    debug!("Performing symlink target deletion...");
    filesystem::remove_file(&symlink.target.target).context("remove symlink")?;
    filesystem::delete_parents(&symlink.target.target, interactive)
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
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Deleting {} but target doesn't exist. Deleting cache anyways.",
                template
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
        TemplateComparison::Changed => {
            if force {
                error!(
                    "Deleting {} but target contents were changed. Skipping...",
                    template
                );
                Ok(false)
            } else {
                warn!(
                    "Deleting {} but target contents were changed. Forcing.",
                    template
                );
                if act {
                    perform_cache_deletion(template).context("perform cache deletion")?;
                    perform_template_target_deletion(template, interactive)
                        .context("perform template target deletion")?;
                }
                Ok(true)
            }
        }
        TemplateComparison::Identical => {
            debug!("Performing deletion");
            if act {
                perform_cache_deletion(template).context("perform cache deletion")?;
                perform_template_target_deletion(template, interactive)
                    .context("perform template target deletion")?;
            }
            Ok(true)
        }
    }
}

fn perform_cache_deletion(template: &TemplateDescription) -> Result<()> {
    debug!("Performing cache deletion...");
    fs::remove_file(&template.cache).context("delete template cache")?;
    filesystem::delete_parents(&template.cache, false)
        .context("delete parent directory in cache")?;
    Ok(())
}

fn perform_template_target_deletion(
    template: &TemplateDescription,
    interactive: bool,
) -> Result<()> {
    debug!("Performing template target deletion...");
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
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!("Creating {} but source is missing. Skipping...", symlink);
            Ok(false)
        }
        SymlinkComparison::Identical => {
            warn!("Creating {} but target already exists and points at source. Adding to cache anyways", symlink);
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if !force => {
            error!(
                "Creating {} but target already exists and differs from expected. Skipping...",
                symlink
            );
            Ok(false)
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!(
                    "Creating {} but target already exists and differs from expected. Forcing.",
                    symlink
                );
                std::fs::remove_file(&symlink.target.target)
                    .context("remove symlink target while forcing")?;
            }

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
    }
}

// Returns true if the template should be added to cache
fn create_template(
    act: bool,
    template: &TemplateDescription,
    handlebars: &Handlebars,
    variables: &Variables,
    force: bool,
) -> Result<bool> {
    info!("{} {}", "[+]".green(), template);

    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyTargetExists if !force => {
            error!(
                "Creating {} but target file already exists. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::OnlyCacheExists
                || t == TemplateComparison::Identical
                || t == TemplateComparison::Changed
            {
                warn!(
                    "Creating {} but cache file already exists. This is probably a result of an error in the last run.",
                    template
                );
            }
            if t == TemplateComparison::OnlyTargetExists {
                warn!(
                    "Creating {} but target file already exists. Forcing.",
                    template
                );
                filesystem::remove_file(&template.target.target)
                    .context("remove existing file while forcing")?;
            }

            debug!("Performing creation");
            if act {
                perform_template_cache(template, handlebars, variables)
                    .context("perform template cache")?;
                filesystem::create_dir_all(
                    &template
                        .target
                        .target
                        .parent()
                        .context("get parent of target file")?,
                    &template.target.owner,
                )
                .context("create parent for target file")?;
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
            }
            Ok(true)
        }
    }
}

// Returns true if the symlink wasn't skipped
fn update_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    debug!("Updating {}...", symlink);
    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!("Updating {} but source is missing. Skipping...", symlink);
            Ok(false)
        }
        SymlinkComparison::Changed if !force => {
            error!(
                "Updating {} but target doesn't point at source. Skipping...",
                symlink
            );
            Ok(false)
        }
        SymlinkComparison::TargetNotSymlink if !force => {
            error!(
                "Updating {} but target is not a symlink. Skipping...",
                symlink
            );
            Ok(false)
        }
        s => {
            debug!("Performing update");
            if act {
                if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                    warn!(
                        "Updating {} but target wasn't what was expected. Forcing.",
                        symlink
                    );
                    filesystem::remove_file(&symlink.target.target)
                        .context("remove symlink target while forcing")?;
                } else if s == SymlinkComparison::OnlySourceExists {
                    warn!(
                        "Updating {} but target was missing. Creating it anyways.",
                        symlink
                    );
                }
                if !symlink.target.target.exists() {
                    fs::create_dir_all(
                        &symlink
                            .target
                            .target
                            .parent()
                            .context("get parent of target file")?,
                    )
                    .context("create parent for target file")?;
                    filesystem::make_symlink(
                        &symlink.target.target,
                        &symlink.source,
                        &symlink.target.owner,
                    )
                    .context("create target symlink")?;
                } else if !filesystem::is_owned_by_user(&symlink.target.target)
                    .context("detect if target file is owned by the current user")?
                    || symlink.target.owner.is_some()
                {
                    // Must be identical, just update owner
                    filesystem::set_owner(&symlink.target.target, &symlink.target.owner)
                        .context("set target symlink owner")?;
                }
            }
            Ok(true)
        }
    }
}

/// Returns true if the template was not skipped
fn update_template(
    act: bool,
    template: &TemplateDescription,
    handlebars: &Handlebars,
    variables: &Variables,
    force: bool,
    diff_context_lines: usize,
) -> Result<bool> {
    debug!("Updating {}...", template);
    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Updating {} but cache is missing. Cache is CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(true)
        }
        TemplateComparison::Changed if !force => {
            error!(
                "Updating {} but target's contents were changed. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::Changed {
                warn!(
                    "Updating {} but target's contents were changed. Forcing.",
                    template
                );
            }

            if log_enabled!(log::Level::Info) {
                match difference::generate_diff(&template, handlebars, &variables) {
                    Ok(diff) => {
                        if difference::diff_nonempty(&diff) {
                            info!("{} {}", "[~]".yellow(), template);
                            difference::print_diff(diff, diff_context_lines);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to generate diff for {} on step: {}", template, e);
                    }
                }
            }

            debug!("Performing update");
            if act {
                perform_template_cache(template, handlebars, variables)
                    .context("perform template cache")?;

                // Handle TemplateComparison::TargetMissing
                if !template.target.target.exists() {
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
                } else if !filesystem::is_owned_by_user(&template.target.target)
                    .context("detect if target file is owned by the current user")?
                    || template.target.owner.is_some()
                {
                    filesystem::set_owner(&template.target.target, &template.target.owner)
                        .context("set target file owner")?;
                }
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
            }
            Ok(true)
        }
    }
}

fn perform_template_cache(
    template: &TemplateDescription,
    handlebars: &Handlebars,
    variables: &Variables,
) -> Result<()> {
    let file_contents =
        fs::read_to_string(&template.source).context("read template source file")?;
    let file_contents = template.apply_actions(file_contents);
    let rendered = handlebars
        .render_template(&file_contents, variables)
        .context("render template")?;

    fs::create_dir_all(
        &template
            .cache
            .parent()
            .context("get parent of cache file")?,
    )
    .context("create parent for cache file")?;
    fs::write(&template.cache, rendered).context("write rendered template to cache")?;

    Ok(())
}

fn is_template(source: &Path) -> Result<bool> {
    let mut file = File::open(source).context("open file")?;
    let mut buf = String::new();
    if file.read_to_string(&mut buf).is_err() {
        warn!("File {:?} is not valid UTF-8 - detecting as symlink. Explicitly specify it to silence this message.", source);
        Ok(false)
    } else {
        Ok(buf.contains("{{"))
    }
}
