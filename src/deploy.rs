use anyhow::{Context, Result};

use handlebars::Handlebars;

use std::collections::BTreeMap;
use std::io::{self, Read};

use crate::actions::Action;
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

    let plan = plan_deploy(state);
    let mut fs = crate::filesystem::RealFilesystem::new(opt.interactive);

    for action in plan {
        match action.run(&mut fs, opt, &handlebars, &variables) {
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

    let plan = plan_deploy(state);
    let mut fs = crate::filesystem::RealFilesystem::new(opt.interactive);

    for action in plan {
        match action.run(&mut fs, &opt, &handlebars, &variables) {
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

#[cfg(test)]
mod test {
    use crate::{
        config::{SymbolicTarget, TemplateTarget},
        filesystem::SymlinkComparison,
    };
    use crate::{
        file_state::{SymlinkDescription, TemplateDescription},
        filesystem::TemplateComparison,
    };

    use std::{
        collections::BTreeSet,
        path::{Path, PathBuf},
    };

    use super::*;

    use mockall::predicate::*;

    #[test]
    fn initial_deploy() {
        // File state
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

        // Plan
        let actions = plan_deploy(file_state);
        assert_eq!(
            actions,
            [Action::CreateSymlink(a), Action::CreateTemplate(b)]
        );

        // Setup
        let mut fs = crate::filesystem::MockFilesystem::new();
        let mut seq = mockall::Sequence::new();

        let options = Options::default();
        let handlebars = handlebars::Handlebars::new();
        let variables = Default::default();

        fn path_eq(expected: &str) -> impl Fn(&Path) -> bool {
            let expected = PathBuf::from(expected);
            move |actual| actual == expected
        }

        // Action 1
        fs.expect_compare_symlink()
            .times(1)
            .with(function(path_eq("a_in")), function(path_eq("a_out")))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(SymlinkComparison::OnlySourceExists));
        fs.expect_create_dir_all()
            .times(1)
            .with(function(path_eq("")), eq(None)) // parent of a_out
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        fs.expect_make_symlink()
            .times(1)
            .with(
                function(path_eq("a_out")),
                function(path_eq("a_in")),
                eq(None),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(()));

        actions[0]
            .run(&mut fs, &options, &handlebars, &variables)
            .unwrap();

        fs.checkpoint();

        // Action 2
        fs.expect_compare_template()
            .times(1)
            .with(
                function(path_eq("b_out")),
                function(path_eq("cache/b_cache")),
            )
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(TemplateComparison::BothMissing));
        fs.expect_create_dir_all()
            .times(1)
            .with(function(path_eq("")), eq(None)) // parent of b_out
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        fs.expect_read_to_string()
            .times(1)
            .with(function(path_eq("b_in")))
            .in_sequence(&mut seq)
            .returning(|_| Ok("".into()));
        fs.expect_create_dir_all()
            .times(1)
            .with(function(path_eq("cache")), eq(None))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        fs.expect_write()
            .times(1)
            .with(function(path_eq("cache/b_cache")), eq(String::from("")))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        fs.expect_copy_file()
            .times(1)
            .with(
                function(path_eq("cache/b_cache")),
                function(path_eq("b_out")),
                eq(None),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(()));
        fs.expect_copy_permissions()
            .times(1)
            .with(
                function(path_eq("b_in")),
                function(path_eq("b_out")),
                eq(None),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(()));

        actions[1]
            .run(&mut fs, &options, &handlebars, &variables)
            .unwrap();
    }
}
