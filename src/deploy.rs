use anyhow::{Context, Result};

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::PathBuf;

use crate::actions::{self, ActionRunner, RealActionRunner};
use crate::args::Options;
use crate::config::{self, Cache, FileTarget, SymbolicTarget, TemplateTarget};
use crate::display_error;
use crate::filesystem::{self, load_file, Filesystem};
use crate::handlebars_helpers::create_new_handlebars;
use crate::hooks;

/// Returns true if an error was printed
pub fn deploy(opt: &Options) -> Result<bool> {
    // === Load configuration ===
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

    // === Pre-deploy ===

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

    let (mut real_fs, mut dry_run_fs);
    let fs: &mut dyn Filesystem = if opt.act {
        real_fs = crate::filesystem::RealFilesystem::new(opt.interactive);
        &mut real_fs
    } else {
        dry_run_fs = crate::filesystem::DryRunFilesystem::new();
        &mut dry_run_fs
    };

    // === Re-structure configuration ===

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
                    } else {
                        desired_symlinks.insert(source, target.into());
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

    // === Perform deployment ===

    let mut runner = RealActionRunner::new(
        fs,
        &handlebars,
        &config.variables,
        opt.force,
        opt.diff_context_lines,
    );

    let (suggest_force, mut error_occurred) = run_deploy(
        &mut runner,
        &desired_symlinks,
        &desired_templates,
        &mut cache,
        &opt,
    )
    .context("run deploy")?;

    // === Post-deploy ===

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
    // === Load configuration ===
    let mut config = config::load_configuration(&opt.local_config, &opt.global_config, None)
        .context("get a configuration")?;

    let mut cache: config::Cache = filesystem::load_file(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let handlebars = create_new_handlebars(&mut config);

    // === Pre-undeploy ===

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

    // === Perform undeployment ===

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

    // === Post-undeploy ===

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

fn run_deploy<A: ActionRunner>(
    runner: &mut A,
    desired_symlinks: &BTreeMap<PathBuf, SymbolicTarget>,
    desired_templates: &BTreeMap<PathBuf, TemplateTarget>,
    cache: &mut Cache,
    opt: &Options,
) -> Result<(bool, bool)> {
    let mut suggest_force = false;
    let mut error_occurred = false;

    fn difference<T1, T2>(
        map1: &BTreeMap<PathBuf, T1>,
        map2: &BTreeMap<PathBuf, T2>,
    ) -> BTreeSet<PathBuf> {
        let keys1 = map1.keys().collect::<BTreeSet<_>>();
        let keys2 = map2.keys().collect::<BTreeSet<_>>();
        keys1.difference(&keys2).cloned().cloned().collect()
    }

    let mut resulting_cache = cache.clone();

    for deleted_symlink in difference(&cache.symlinks, &desired_symlinks) {
        let target = cache.symlinks.get(&deleted_symlink).unwrap().clone();
        execute_action(
            runner.delete_symlink(&deleted_symlink, &target),
            || resulting_cache.symlinks.remove(&deleted_symlink),
            || format!("delete symlink {:?} -> {:?}", deleted_symlink, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for deleted_template in difference(&cache.templates, &desired_templates) {
        let target = cache.templates.get(&deleted_template).unwrap().clone();
        execute_action(
            runner.delete_template(
                &deleted_template,
                &opt.cache_directory.join(&deleted_template),
                &target,
            ),
            || resulting_cache.templates.remove(&deleted_template),
            || format!("delete template {:?} -> {:?}", deleted_template, target),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for created_symlink in difference(&desired_symlinks, &cache.symlinks) {
        let target = desired_symlinks.get(&created_symlink).unwrap().clone();
        execute_action(
            runner.create_symlink(&created_symlink, &target),
            || {
                resulting_cache
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
            runner.create_template(
                &created_template,
                &opt.cache_directory.join(&created_template),
                &target,
            ),
            || {
                resulting_cache
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
            runner.update_symlink(&updated_symlink, &target),
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
            runner.update_template(
                &updated_template,
                &opt.cache_directory.join(&updated_template),
                &target,
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

    *cache = resulting_cache;

    Ok((suggest_force, error_occurred))
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
    use crate::filesystem::{SymlinkComparison, TemplateComparison};

    use std::path::{Path, PathBuf};

    use super::*;

    use mockall::predicate::*;

    fn path_eq(expected: &str) -> impl Fn(&Path) -> bool {
        let expected = PathBuf::from(expected);
        move |actual| actual == expected
    }

    #[test]
    fn high_level_simple() {
        // State
        let a_out: SymbolicTarget = "a_out".into();
        let b_out: TemplateTarget = "b_out".into();

        let desired_symlinks = maplit::btreemap! {
            PathBuf::from("a_in") => a_out.clone()
        };
        let desired_templates = maplit::btreemap! {
            PathBuf::from("b_in") => b_out.clone()
        };

        // Test high level
        let mut runner = actions::MockActionRunner::new();
        let mut seq = mockall::Sequence::new();
        let mut cache = Cache::default();

        runner
            .expect_create_symlink()
            .times(1)
            .with(function(path_eq("a_in")), eq(a_out))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(true));
        runner
            .expect_create_template()
            .times(1)
            .with(
                function(path_eq("b_in")),
                function(path_eq("cache/b_in")),
                eq(b_out),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(true));

        let (suggest_force, error_occurred) = run_deploy(
            &mut runner,
            &desired_symlinks,
            &desired_templates,
            &mut cache,
            &Options {
                cache_directory: "cache".into(),
                force: false,
                ..Options::default()
            },
        )
        .unwrap();

        assert_eq!(suggest_force, false);
        assert_eq!(error_occurred, false);

        assert!(cache.symlinks.contains_key(&PathBuf::from("a_in")));
        assert!(cache.templates.contains_key(&PathBuf::from("b_in")));
        assert_eq!(cache.symlinks.len(), 1);
        assert_eq!(cache.templates.len(), 1);
    }

    #[test]
    fn high_level_skip() {
        // Setup
        let a_out: SymbolicTarget = "a_out".into();
        let b_out: TemplateTarget = "b_out".into();

        let desired_symlinks = maplit::btreemap! {
            PathBuf::from("a_in") => a_out.clone()
        };
        let desired_templates = maplit::btreemap! {
            PathBuf::from("b_in") => b_out.clone()
        };

        let mut runner = actions::MockActionRunner::new();
        let mut seq = mockall::Sequence::new();
        let mut cache = Cache::default();

        // Expectation
        runner
            .expect_create_symlink()
            .times(1)
            .with(function(path_eq("a_in")), eq(a_out))
            .in_sequence(&mut seq)
            .returning(|_, _| Err(anyhow::anyhow!("oh no")));
        runner
            .expect_create_template()
            .times(1)
            .with(
                function(path_eq("b_in")),
                function(path_eq("cache/b_in")),
                eq(b_out),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(false));

        // Reality
        let (suggest_force, error_occurred) = run_deploy(
            &mut runner,
            &desired_symlinks,
            &desired_templates,
            &mut cache,
            &Options {
                cache_directory: "cache".into(),
                force: false,
                ..Options::default()
            },
        )
        .unwrap();

        assert_eq!(suggest_force, true);
        assert_eq!(error_occurred, true);

        assert_eq!(cache.symlinks.len(), 0);
        assert_eq!(cache.templates.len(), 0);
    }

    #[test]
    fn low_level_simple() {
        // Setup
        let mut fs = crate::filesystem::MockFilesystem::new();
        let mut seq = mockall::Sequence::new();

        let opt = Options::default();
        let handlebars = handlebars::Handlebars::new();
        let variables = Default::default();

        // Expectation:
        // create_symlink
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

        // create_template
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
            .returning(|_| Ok("Hello!".into()));
        fs.expect_create_dir_all()
            .times(1)
            .with(function(path_eq("cache")), eq(None))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(()));
        fs.expect_write()
            .times(1)
            .with(
                function(path_eq("cache/b_cache")),
                eq(String::from("Hello!")),
            )
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

        // Reality
        let mut runner = actions::RealActionRunner::new(
            &mut fs,
            &handlebars,
            &variables,
            opt.force,
            opt.diff_context_lines,
        );
        assert!(runner
            .create_symlink(&PathBuf::from("a_in"), &PathBuf::from("a_out").into())
            .unwrap());
        assert!(runner
            .create_template(
                &PathBuf::from("b_in"),
                &PathBuf::from("cache/b_cache"),
                &PathBuf::from("b_out").into(),
            )
            .unwrap());
    }

    #[test]
    fn low_level_skip() {
        // Setup
        let mut fs = crate::filesystem::MockFilesystem::new();
        let mut seq = mockall::Sequence::new();

        let opt = Options::default();
        let handlebars = handlebars::Handlebars::new();
        let variables = Default::default();

        // Expectation:
        // create_symlink
        fs.expect_compare_symlink()
            .times(1)
            .with(function(path_eq("a_in")), function(path_eq("a_out")))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(SymlinkComparison::Changed));

        // create_template
        fs.expect_compare_template()
            .times(1)
            .with(
                function(path_eq("b_out")),
                function(path_eq("cache/b_cache")),
            )
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(TemplateComparison::Changed));

        // Reality
        let mut runner = actions::RealActionRunner::new(
            &mut fs,
            &handlebars,
            &variables,
            opt.force,
            opt.diff_context_lines,
        );

        // Both should skip
        assert!(!runner
            .create_symlink(&PathBuf::from("a_in"), &PathBuf::from("a_out").into())
            .unwrap());
        assert!(!runner
            .create_template(
                &PathBuf::from("b_in"),
                &PathBuf::from("cache/b_cache"),
                &PathBuf::from("b_out").into(),
            )
            .unwrap());
    }
}
