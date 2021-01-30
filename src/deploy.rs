use anyhow::{Context, Result};

use handlebars::Handlebars;

use std::collections::BTreeMap;
use std::io::{self, Read};

use crate::args::Options;
use crate::config;
use crate::display_error;
use crate::file_state::{file_state_from_configuration, FileState};
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

    let mut cache = if let Some(cache) = config::load_cache(&opt.cache_file)? {
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
    let mut fs = crate::filesystem::RealFilesystem::new(opt.interactive);

    for action in plan {
        match action.run(&mut fs, opt) {
            Ok(true) => action.affect_cache(&mut cache),
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                error_occurred = true;
                display_error(e);
            }
        }
    }

    trace!("Actual symlinks: {:#?}", cache.symlinks);
    trace!("Actual templates: {:#?}", cache.templates);

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
        error_occurred = true;
    }

    if opt.act {
        config::save_cache(&opt.cache_file, cache)?;
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

pub fn undeploy(opt: Options) -> Result<bool> {
    let config = config::load_configuration(&opt.local_config, &opt.global_config, None)
        .context("get a configuration")?;

    let mut cache = config::load_cache(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    // Used just to transform them into Description structs
    let state = FileState::new(
        BTreeMap::default(),
        BTreeMap::default(),
        cache.symlinks.clone(),
        cache.templates.clone(),
        opt.cache_directory.clone(),
    );
    trace!("File state: {:#?}", state);

    let config::Configuration {
        files,
        mut variables,
        helpers,
        packages,
    } = config;

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

    let mut suggest_force = false;
    let mut error_occurred = false;

    let plan = crate::actions::plan_deploy(state);
    let mut fs = crate::filesystem::RealFilesystem::new(opt.interactive);

    for action in plan {
        match action.run(&mut fs, &opt) {
            Ok(true) => action.affect_cache(&mut cache),
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                error_occurred = true;
                display_error(e);
            }
        }
    }

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
        error_occurred = true;
    }

    if opt.act {
        // Should be empty if everything went well, but if some things were skipped this contains
        // them.
        config::save_cache(&opt.cache_file, cache)?;
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

    Ok(error_occurred)
}
