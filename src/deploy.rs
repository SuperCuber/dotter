use anyhow::{Context, Result};

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::PathBuf;

use crate::actions::{self, ActionRunner, RealActionRunner};
use crate::args::Options;
use crate::config::{self, Cache, FileTarget, SymbolicTarget, TemplateTarget};
use crate::display_error;
use crate::filesystem::{self, Filesystem, load_file};
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

    let handlebars = create_new_handlebars(&mut config).context("initialize handlebars")?;

    debug!("Running pre-deploy hook");
    if !opt.dry_run {
        hooks::run_hook(
            &opt.pre_deploy,
            &opt.cache_directory,
            &handlebars,
            &config.variables,
        )
        .context("run pre-deploy hook")?;
    }

    let (mut real_fs, mut dry_run_fs);
    let fs: &mut dyn Filesystem = if !opt.dry_run {
        real_fs = crate::filesystem::RealFilesystem::new(opt.noconfirm);
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
                    if filesystem::is_template(&source)
                        .context(format!("check whether {source:?} is a template"))?
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
        opt,
    );

    // === Post-deploy ===

    if suggest_force {
        error!(
            "Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag."
        );
        error_occurred = true;
    }

    if !opt.dry_run {
        filesystem::save_file(&opt.cache_file, cache).context("save cache")?;
    }

    debug!("Running post-deploy hook");
    if !opt.dry_run {
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

pub fn undeploy(opt: &Options) -> Result<bool> {
    // === Load configuration ===
    let mut config = config::load_configuration(&opt.local_config, &opt.global_config, None)
        .context("get a configuration")?;

    let mut cache: config::Cache = filesystem::load_file(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let handlebars = create_new_handlebars(&mut config).context("initialize handlebars")?;

    // === Pre-undeploy ===

    debug!("Running pre-undeploy hook");
    if !opt.dry_run {
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
    let fs: &mut dyn Filesystem = if !opt.dry_run {
        real_fs = crate::filesystem::RealFilesystem::new(opt.noconfirm);
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
            || format!("delete symlink {deleted_symlink:?} -> {target:?}"),
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
            || format!("delete template {deleted_template:?} -> {target:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    // === Post-undeploy ===

    if suggest_force {
        error!(
            "Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag."
        );
        error_occurred = true;
    }

    if !opt.dry_run {
        // Should be empty if everything went well, but if some things were skipped this contains
        // them.
        filesystem::save_file(&opt.cache_file, cache).context("save cache")?;
    }

    debug!("Running post-undeploy hook");
    if !opt.dry_run {
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
) -> (bool, bool) {
    let mut suggest_force = false;
    let mut error_occurred = false;

    // Index by both source and target location
    let existing_symlinks: BTreeSet<(PathBuf, PathBuf)> = cache
        .symlinks
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let existing_templates: BTreeSet<(PathBuf, PathBuf)> = cache
        .templates
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let desired_symlinks: BTreeMap<(PathBuf, PathBuf), _> = desired_symlinks
        .iter()
        .map(|(k, v)| ((k.clone(), v.target.clone()), v))
        .collect();
    let desired_templates: BTreeMap<(PathBuf, PathBuf), _> = desired_templates
        .iter()
        .map(|(k, v)| ((k.clone(), v.target.clone()), v))
        .collect();

    // Avoid modifying cache while iterating over it
    let mut resulting_cache = cache.clone();

    for (source, target) in
        existing_symlinks.difference(&desired_symlinks.keys().cloned().collect())
    {
        execute_action(
            runner.delete_symlink(source, target),
            || resulting_cache.symlinks.remove(source),
            || format!("delete symlink {source:?} -> {target:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (source, target) in
        existing_templates.difference(&desired_templates.keys().cloned().collect())
    {
        execute_action(
            runner.delete_template(source, &opt.cache_directory.join(source), target),
            || resulting_cache.templates.remove(source),
            || format!("delete template {source:?} -> {target:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (source, target_path) in desired_symlinks
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>()
        .difference(&existing_symlinks)
    {
        let target = desired_symlinks
            .get(&(source.into(), target_path.into()))
            .unwrap();
        execute_action(
            runner.create_symlink(source, target),
            || {
                resulting_cache
                    .symlinks
                    .insert(source.clone(), target_path.clone())
            },
            || format!("create symlink {source:?} -> {target_path:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (source, target_path) in desired_templates
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>()
        .difference(&existing_templates)
    {
        let target = desired_templates
            .get(&(source.into(), target_path.into()))
            .unwrap();
        execute_action(
            runner.create_template(source, &opt.cache_directory.join(source), target),
            || {
                resulting_cache
                    .templates
                    .insert(source.clone(), target_path.clone())
            },
            || format!("create template {source:?} -> {target_path:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (source, target_path) in
        existing_symlinks.intersection(&desired_symlinks.keys().cloned().collect())
    {
        let target = desired_symlinks
            .get(&(source.into(), target_path.into()))
            .unwrap();
        execute_action(
            runner.update_symlink(source, target),
            || (),
            || format!("update symlink {source:?} -> {target_path:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    for (source, target_path) in
        existing_templates.intersection(&desired_templates.keys().cloned().collect())
    {
        let target = desired_templates
            .get(&(source.into(), target_path.into()))
            .unwrap();
        execute_action(
            runner.update_template(source, &opt.cache_directory.join(source), target),
            || (),
            || format!("update template {source:?} -> {target_path:?}"),
            &mut suggest_force,
            &mut error_occurred,
        );
    }

    *cache = resulting_cache;

    (suggest_force, error_occurred)
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
        );

        assert!(!suggest_force);
        assert!(!error_occurred);

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
        );

        assert!(suggest_force);
        assert!(error_occurred);

        assert_eq!(cache.symlinks.len(), 0);
        assert_eq!(cache.templates.len(), 0);
    }

    #[test]
    fn high_level_change_target() {
        // Setup
        let a_out_new: SymbolicTarget = "a_out_new".into();

        let desired_symlinks = maplit::btreemap! {
            PathBuf::from("a_in") => a_out_new.clone()
        };

        let mut runner = actions::MockActionRunner::new();
        let mut seq = mockall::Sequence::new();
        let mut cache = Cache {
            symlinks: maplit::btreemap! {
                PathBuf::from("a_in") => "a_out_old".into()
            },
            templates: BTreeMap::new(),
        };

        // Expectation
        runner
            .expect_delete_symlink()
            .times(1)
            .with(function(path_eq("a_in")), function(path_eq("a_out_old")))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(true));
        runner
            .expect_create_symlink()
            .times(1)
            .with(function(path_eq("a_in")), eq(a_out_new))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(true));

        // Reality
        let (suggest_force, error_occurred) = run_deploy(
            &mut runner,
            &desired_symlinks,
            &BTreeMap::new(),
            &mut cache,
            &Options {
                cache_directory: "cache".into(),
                force: false,
                ..Options::default()
            },
        );

        assert!(!suggest_force);
        assert!(!error_occurred);

        assert_eq!(cache.symlinks.len(), 1);
        assert_eq!(cache.templates.len(), 0);
    }

    #[test]
    fn high_level_change_type() {
        // Setup
        let a_out_new: SymbolicTarget = "a_out_new".into();

        let desired_symlinks = maplit::btreemap! {
            PathBuf::from("a_in") => a_out_new.clone()
        };

        let mut runner = actions::MockActionRunner::new();
        let mut seq = mockall::Sequence::new();
        let mut cache = Cache {
            symlinks: BTreeMap::new(),
            templates: maplit::btreemap! {
                PathBuf::from("a_in") => "a_out_old".into()
            },
        };

        // Expectation
        runner
            .expect_delete_template()
            .times(1)
            .with(
                function(path_eq("a_in")),
                function(path_eq("cache/a_in")),
                function(path_eq("a_out_old")),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(true));
        runner
            .expect_create_symlink()
            .times(1)
            .with(function(path_eq("a_in")), eq(a_out_new))
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(true));

        // Reality
        let (suggest_force, error_occurred) = run_deploy(
            &mut runner,
            &desired_symlinks,
            &BTreeMap::new(),
            &mut cache,
            &Options {
                cache_directory: "cache".into(),
                force: false,
                ..Options::default()
            },
        );

        assert!(!suggest_force);
        assert!(!error_occurred);

        assert_eq!(cache.symlinks.len(), 1);
        assert_eq!(cache.templates.len(), 0);
    }

    #[test]
    #[ignore] // This is desired, but not implemented: see issue #22
    fn high_level_skip_change_type() {
        // Setup
        let desired_symlinks = maplit::btreemap! {
            PathBuf::from("a_in") => "a_out_new".into()
        };

        let mut runner = actions::MockActionRunner::new();
        let mut seq = mockall::Sequence::new();
        let mut cache = Cache {
            symlinks: BTreeMap::new(),
            templates: maplit::btreemap! {
                PathBuf::from("a_in") => "a_out_old".into()
            },
        };

        // Expectation
        runner
            .expect_delete_template()
            .times(1)
            .with(
                function(path_eq("a_in")),
                function(path_eq("cache/a_in")),
                function(path_eq("a_out_old")),
            )
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(false));

        // Reality
        let (suggest_force, error_occurred) = run_deploy(
            &mut runner,
            &desired_symlinks,
            &BTreeMap::new(),
            &mut cache,
            &Options {
                cache_directory: "cache".into(),
                force: false,
                ..Options::default()
            },
        );

        assert!(!suggest_force);
        assert!(!error_occurred);

        assert_eq!(cache.symlinks.len(), 1);
        assert_eq!(cache.templates.len(), 0);
    }

    #[test]
    fn low_level_simple() {
        // Setup
        let mut fs = crate::filesystem::MockFilesystem::new();
        let mut seq = mockall::Sequence::new();

        let opt = Options::default();
        let handlebars = handlebars::Handlebars::new();
        let variables = toml::map::Map::new();

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
        assert!(
            runner
                .create_symlink(&PathBuf::from("a_in"), &PathBuf::from("a_out").into())
                .unwrap()
        );
        assert!(
            runner
                .create_template(
                    &PathBuf::from("b_in"),
                    &PathBuf::from("cache/b_cache"),
                    &PathBuf::from("b_out").into(),
                )
                .unwrap()
        );
    }

    #[test]
    fn low_level_skip() {
        // Setup
        let mut fs = crate::filesystem::MockFilesystem::new();
        let mut seq = mockall::Sequence::new();

        let opt = Options::default();
        let handlebars = handlebars::Handlebars::new();
        let variables = toml::map::Map::new();

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
        assert!(
            !runner
                .create_symlink(&PathBuf::from("a_in"), &PathBuf::from("a_out").into())
                .unwrap()
        );
        assert!(
            !runner
                .create_template(
                    &PathBuf::from("b_in"),
                    &PathBuf::from("cache/b_cache"),
                    &PathBuf::from("b_out").into(),
                )
                .unwrap()
        );
    }
}
