use anyhow::{Context, Result};
use crossterm::style::Colorize;

use handlebars::Handlebars;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};

use crate::args::Options;
use crate::config::{self, Variables};
use crate::difference;
use crate::display_error;
use crate::file_state::{
    file_state_from_configuration, FileState, SymlinkDescription, TemplateDescription,
};
use crate::filesystem::{self, SymlinkComparison, TemplateComparison};
use crate::handlebars_helpers;
use crate::hooks;

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

    debug!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, &helpers);
    handlebars_helpers::add_dotter_variable(&mut variables, &files, &packages);
    trace!("Handlebars instance: {:#?}", handlebars);

    debug!("Running pre-deploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.pre_deploy,
            &opt.cache_directory,
            &handlebars,
            &variables,
        )
        .context("run pre-deploy hook")?;
    }

    let mut suggest_force = false;
    let mut error_occurred = false;

    let plan = crate::actions::plan_deploy(state);

    let mut fs = crate::filesystem::RealFilesystem::new();

    for action in plan {
        action.run(&mut fs, opt);
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

    debug!("Running post-deploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.post_deploy,
            &opt.cache_directory,
            &handlebars,
            &variables,
        )
        .context("run post-deploy hook")?;
    }

    Ok(error_occurred)
}

pub fn undeploy(opt: Options) -> Result<()> {
    let config = config::load_configuration(&opt.local_config, &opt.global_config, None)
        .context("get a configuration")?;

    let cache = config::load_cache(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let config::Configuration {
        files,
        mut variables,
        helpers,
        packages,
    } = config;

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
        opt.cache_directory.clone(),
    );
    trace!("File state: {:#?}", state);

    debug!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, &helpers);
    handlebars_helpers::add_dotter_variable(&mut variables, &files, &packages);
    trace!("Handlebars instance: {:#?}", handlebars);

    debug!("Running pre-undeploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.pre_undeploy,
            &opt.cache_directory,
            &handlebars,
            &variables,
        )
        .context("run pre-undeploy hook")?;
    }

    let (deleted_symlinks, deleted_templates) = state.deleted_files();

    let mut actual_symlinks = existing_symlinks;
    let mut actual_templates = existing_templates;
    let mut suggest_force = false;

    let plan = crate::actions::plan_deploy(state);

    let mut fs = crate::filesystem::RealFilesystem::new();

    for action in plan {
        action.run(&mut fs, &opt);
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

    debug!("Running post-undeploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.post_undeploy,
            &opt.cache_directory,
            &handlebars,
            &variables,
        )
        .context("run post-undeploy hook")?;
    }

    Ok(())
}

// == DELETE ==

/// Returns true if symlink should be deleted from cache
pub fn delete_symlink(
    symlink: &SymlinkDescription,
    fs: &mut impl crate::filesystem::Filesystem,
    act: bool,
    force: bool,
    interactive: bool,
) -> Result<bool> {
    info!("{} {}", "[-]".red(), symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target.target)
        .context("detect symlink's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::Identical | SymlinkComparison::OnlyTargetExists => {
            debug!("Performing deletion");
            if act {
                perform_symlink_target_deletion(symlink, interactive)
                    .context("perform symlink target deletion")?;
            }
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
            // -f > -v
            perform_symlink_target_deletion(symlink, interactive)
                .context("perform symlink target deletion")?;
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink => {
            error!("Deleting {} but {}. Skipping.", symlink, comparison);
            Ok(false)
        }
    }
}

fn perform_symlink_target_deletion(symlink: &SymlinkDescription, interactive: bool) -> Result<()> {
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
