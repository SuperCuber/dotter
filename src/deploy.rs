use anyhow::{Context, Result};

use config::{FileTarget, SymbolicTarget, TemplateTarget};
use filesystem::load_file;
use handlebars_helpers::create_new_handlebars;

use std::collections::BTreeSet;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
};

use crate::actions;
use crate::args::Options;
use crate::config;
use crate::display_error;
use crate::filesystem::{self, Filesystem};
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

    let mut config = config::load_configuration(&opt.local_config, &opt.global_config, patch)
        .context("get a configuration")?;

    let mut cache = if let Some(cache) = load_file(&opt.cache_file)? {
        cache
    } else {
        warn!("Cache file not found. Assuming cache is empty.");
        config::Cache::default()
    };

    let handlebars = create_new_handlebars(&mut config);

    debug!("Running pre-deploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.pre_deploy,
            &opt.cache_directory,
            &handlebars,
            &config.variables,
        )
        .context("run pre-deploy hook")?;
    }

    let mut suggest_force = false;
    let mut error_occurred = false;

    let (mut real_fs, mut dry_run_fs);
    let fs: &mut dyn Filesystem = if opt.act {
        real_fs = crate::filesystem::RealFilesystem::new(opt.interactive);
        &mut real_fs
    } else {
        dry_run_fs = crate::filesystem::DryRunFilesystem::new();
        &mut dry_run_fs
    };

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

    let mut desired_symlinks = BTreeMap::<PathBuf, SymbolicTarget>::new();
    let mut desired_templates = BTreeMap::<PathBuf, TemplateTarget>::new();

    for (source, target) in config.files {
        if symlinks_enabled {
            match target {
                FileTarget::Automatic(target) => {
                    if fs
                        .is_template(&source)
                        .context(format!("check whether {:?} is a template", source))?
                    {
                        desired_templates.insert(source, target.into());
                    }
                }
                FileTarget::Symbolic(target) => {
                    desired_symlinks.insert(source, target);
                }
                FileTarget::ComplexTemplate(target) => {
                    desired_templates.insert(source, target);
                }
            }
        } else {
            match target {
                FileTarget::Automatic(target) => {
                    desired_templates.insert(source, target.into());
                }
                FileTarget::Symbolic(target) => {
                    desired_templates.insert(source, target.into_template());
                }
                FileTarget::ComplexTemplate(target) => {
                    desired_templates.insert(source, target);
                }
            }
        }
    }

    fn difference<T1, T2>(
        map1: &BTreeMap<PathBuf, T1>,
        map2: &BTreeMap<PathBuf, T2>,
    ) -> BTreeSet<PathBuf> {
        let keys1 = map1.keys().collect::<BTreeSet<_>>();
        let keys2 = map2.keys().collect::<BTreeSet<_>>();
        keys1.difference(&keys2).cloned().cloned().collect()
    }

    for deleted_symlink in difference(&cache.symlinks, &desired_symlinks) {
        let target = cache.symlinks.get(&deleted_symlink).unwrap().clone();
        execute_action(
            actions::delete_symlink(&deleted_symlink, &target, fs, opt.force),
            || cache.symlinks.remove(&deleted_symlink),
            || format!("delete symlink {:?} -> {:?}", deleted_symlink, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for deleted_template in difference(&cache.templates, &desired_templates) {
        let target = cache.templates.get(&deleted_template).unwrap().clone();
        execute_action(
            actions::delete_template(
                &deleted_template,
                &opt.cache_directory.join(&deleted_template),
                &target,
                fs,
                opt.force,
            ),
            || cache.templates.remove(&deleted_template),
            || format!("delete template {:?} -> {:?}", deleted_template, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for created_symlink in difference(&desired_symlinks, &cache.symlinks) {
        let target = desired_symlinks.get(&created_symlink).unwrap().clone();
        execute_action(
            actions::create_symlink(&created_symlink, &target, fs, opt.force),
            || {
                cache
                    .symlinks
                    .insert(created_symlink.clone(), target.target.clone())
            },
            || {
                format!(
                    "create symlink {:?} -> {:?}",
                    created_symlink, target.target
                )
            },
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for created_template in difference(&desired_templates, &cache.templates) {
        let target = desired_templates.get(&created_template).unwrap().clone();
        execute_action(
            actions::create_template(
                &created_template,
                &target,
                &opt.cache_directory,
                fs,
                &handlebars,
                &config.variables,
                opt.force,
            ),
            || {
                cache
                    .templates
                    .insert(created_template.clone(), target.target.clone())
            },
            || {
                format!(
                    "create template {:?} -> {:?}",
                    created_template, target.target
                )
            },
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    fn intersection<T1, T2>(
        map1: &BTreeMap<PathBuf, T1>,
        map2: &BTreeMap<PathBuf, T2>,
    ) -> BTreeSet<PathBuf> {
        let keys1 = map1.keys().collect::<BTreeSet<_>>();
        let keys2 = map2.keys().collect::<BTreeSet<_>>();
        keys1.intersection(&keys2).cloned().cloned().collect()
    }

    for updated_symlink in intersection(&desired_symlinks, &cache.symlinks) {
        let target = desired_symlinks.get(&updated_symlink).unwrap().clone();
        execute_action(
            actions::update_symlink(&updated_symlink, &target, fs, opt.force),
            || (),
            || {
                format!(
                    "update symlink {:?} -> {:?}",
                    updated_symlink, target.target
                )
            },
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for updated_template in intersection(&desired_templates, &cache.templates) {
        let target = desired_templates.get(&updated_template).unwrap().clone();
        execute_action(
            actions::update_template(
                &updated_template,
                &target,
                &opt.cache_directory,
                fs,
                &handlebars,
                &config.variables,
                opt.force,
                opt.diff_context_lines,
            ),
            || (),
            || {
                format!(
                    "update template {:?} -> {:?}",
                    updated_template, target.target
                )
            },
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
        error_occurred = true;
    }

    if opt.act {
        filesystem::save_file(&opt.cache_file, cache).context("save cache")?;
    }

    debug!("Running post-deploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.post_deploy,
            &opt.cache_directory,
            &handlebars,
            &config.variables,
        )
        .context("run post-deploy hook")?;
    }

    Ok(error_occurred)
}

pub fn undeploy(opt: Options) -> Result<bool> {
    let mut config = config::load_configuration(&opt.local_config, &opt.global_config, None)
        .context("get a configuration")?;

    let mut cache: config::Cache = filesystem::load_file(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let handlebars = create_new_handlebars(&mut config);

    debug!("Running pre-undeploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.pre_undeploy,
            &opt.cache_directory,
            &handlebars,
            &config.variables,
        )
        .context("run pre-undeploy hook")?;
    }

    let mut suggest_force = false;
    let mut error_occurred = false;

    let (mut real_fs, mut dry_run_fs);
    let fs: &mut dyn Filesystem = if opt.act {
        real_fs = crate::filesystem::RealFilesystem::new(opt.interactive);
        &mut real_fs
    } else {
        dry_run_fs = crate::filesystem::DryRunFilesystem::new();
        &mut dry_run_fs
    };

    for (deleted_symlink, target) in cache.symlinks.clone() {
        execute_action(
            actions::delete_symlink(&deleted_symlink, &target, fs, opt.force),
            || cache.symlinks.remove(&deleted_symlink),
            || format!("delete symlink {:?} -> {:?}", deleted_symlink, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (deleted_template, target) in cache.templates.clone() {
        execute_action(
            actions::delete_template(
                &deleted_template,
                &opt.cache_directory.join(&deleted_template),
                &target,
                fs,
                opt.force,
            ),
            || cache.templates.remove(&deleted_template),
            || format!("delete template {:?} -> {:?}", deleted_template, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
        error_occurred = true;
    }

    if opt.act {
        // Should be empty if everything went well, but if some things were skipped this contains
        // them.
        filesystem::save_file(&opt.cache_file, cache).context("save cache")?;
    }

    debug!("Running post-undeploy hook");
    if opt.act {
        hooks::run_hook(
            &opt.post_undeploy,
            &opt.cache_directory,
            &handlebars,
            &config.variables,
        )
        .context("run post-undeploy hook")?;
    }

    Ok(error_occurred)
}

/// Used to remove duplication
fn execute_action<T, S: FnOnce() -> T, E: FnOnce() -> String>(
    result: Result<bool>,
    success: S,
    context: E,
    suggest_force: &mut bool,
    error_occurred: &mut bool,
) {
    match result {
        Ok(true) => {
            success();
        }
        Ok(false) => {
            *suggest_force = true;
        }
        Err(e) => {
            display_error(e.context(context()));
            *error_occurred = true;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::filesystem::TemplateComparison;
    use crate::{
        config::{SymbolicTarget, TemplateTarget},
        filesystem::SymlinkComparison,
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
