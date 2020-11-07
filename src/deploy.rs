use anyhow::{Context, Result};

use handlebars::Handlebars;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::display_error;
use args::Options;
use config::{self, Variables};
use filesystem::{self, SymlinkComparison, TemplateComparison};
use handlebars_helpers;

pub fn undeploy(opt: Options) -> Result<()> {
    config::load_configuration(&opt.local_config, &opt.global_config)
        .context("find configuration location")?;

    let cache = config::load_cache(&opt.cache_file)?
        .context("load cache: Cannot undeploy without a cache.")?;

    let config::Cache {
        symlinks: existing_symlinks,
        templates: existing_templates,
    } = cache;

    // Used just to transform them into Description structs
    let state = FileState::new(
        Default::default(),
        Default::default(),
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
        match delete_symlink(opt.act, &symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.remove(&symlink.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete symlink {}", symlink))),
        }
    }

    for template in deleted_templates {
        match delete_template(opt.act, &template, opt.force) {
            Ok(true) => {
                actual_templates.remove(&template.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete template {}", template))),
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

pub fn deploy(opt: Options) -> Result<()> {
    // Throughout this function I'll be referencing steps, those were described in issue #6

    // Step 1
    let (files, variables, helpers) =
        config::load_configuration(&opt.local_config, &opt.global_config)
            .context("get a configuration")?;

    // On Windows, you need developer mode to create symlinks.
    let symlinks_enabled = if filesystem::symlinks_enabled(&PathBuf::from("DOTTER_SYMLINK_TEST"))
        .context("check whether symlinks are enabled")?
    {
        true
    } else {
        error!(
            "No permission to create symbolic links.\n
On Windows, in order to create symbolic links you need to enable Developer Mode.\n
Proceeding by copying instead of symlinking."
        );
        false
    };

    // Step 2-3
    let mut desired_symlinks = BTreeMap::new();
    let mut desired_templates = BTreeMap::new();

    for (source, target) in files {
        match target {
            config::FileTarget::Automatic(target) => {
                if symlinks_enabled
                    && !is_template(&source)
                        .context(format!("check whether {:?} is a template", source))?
                {
                    desired_symlinks.insert(source, target);
                } else {
                    desired_templates.insert(
                        source,
                        config::TemplateTarget {
                            target,
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
                            target,
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

    // Step 4
    let cache = match config::load_cache(&opt.cache_file)? {
        Some(cache) => cache,
        None => {
            warn!("Cache file not found. Assuming cache is empty.");
            Default::default()
        }
    };

    let config::Cache {
        symlinks: existing_symlinks,
        templates: existing_templates,
    } = cache;

    let state = FileState::new(
        desired_symlinks,
        desired_templates,
        existing_symlinks.clone(),
        existing_templates.clone(),
        opt.cache_directory,
    );
    trace!("File state: {:#?}", state);

    let mut actual_symlinks = existing_symlinks;
    let mut actual_templates = existing_templates;
    let mut suggest_force = false;

    // Step 5+6
    let (deleted_symlinks, deleted_templates) = state.deleted_files();
    trace!("Deleted symlinks: {:#?}", deleted_symlinks);
    trace!("Deleted templates: {:#?}", deleted_templates);
    for deleted_symlink in deleted_symlinks {
        match delete_symlink(opt.act, &deleted_symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.remove(&deleted_symlink.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete symlink {}", deleted_symlink))),
        }
    }
    for deleted_template in deleted_templates {
        match delete_template(opt.act, &deleted_template, opt.force) {
            Ok(true) => {
                actual_templates.remove(&deleted_template.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("delete template {}", deleted_template))),
        }
    }

    // Prepare handlebars instance
    info!("Creating Handlebars instance...");
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars.set_strict_mode(true); // Report missing variables as errors
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, helpers);
    trace!("Handlebars instance: {:#?}", handlebars);

    // Step 7+8
    let (new_symlinks, new_templates) = state.new_files();
    trace!("New symlinks: {:#?}", new_symlinks);
    trace!("New templates: {:#?}", new_templates);
    for new_symlink in new_symlinks {
        match create_symlink(opt.act, &new_symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.insert(new_symlink.source, new_symlink.target);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("create symlink {}", new_symlink))),
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
            Err(e) => display_error(e.context(format!("create template {}", new_template))),
        }
    }

    // Step 9+10
    let (old_symlinks, old_templates) = state.old_files();
    trace!("Old symlinks: {:#?}", old_symlinks);
    trace!("Old templates: {:#?}", old_templates);
    for old_symlink in old_symlinks {
        match update_symlink(opt.act, &old_symlink, opt.force) {
            Ok(true) => {}
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("update symlink {}", old_symlink))),
        }
    }
    for old_template in old_templates {
        match update_template(opt.act, &old_template, &handlebars, &variables, opt.force) {
            Ok(true) => {}
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("update template {}", old_template))),
        }
    }

    trace!("Actual symlinks: {:#?}", actual_symlinks);
    trace!("Actual templates: {:#?}", actual_templates);

    if suggest_force {
        error!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
    }

    // Step 11
    if opt.act {
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

/// Returns true if symlink should be deleted from cache
fn delete_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    info!("Deleting {}...", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
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
        SymlinkComparison::Changed if !force => {
            error!(
                "Deleting {} but target doesn't point at source file. Skipping...",
                symlink
            );
            Ok(false)
        }
        SymlinkComparison::TargetNotSymlink if !force => {
            error!(
                "Deleting {} but target isn't a symlink. Skipping...",
                symlink
            );
            Ok(false)
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!(
                    "Deleting {} but target wasn't what was expected. Forcing.",
                    symlink
                );
            }

            debug!("Performing deletion");
            if act {
                fs::remove_file(&symlink.target).context("remove symlink")?;
                filesystem::delete_parents(&symlink.target, true)
                    .context("delete parents of symlink")?;
            }
            Ok(true)
        }
    }
}

/// Returns true if template should be deleted from cache
fn delete_template(act: bool, template: &TemplateDescription, force: bool) -> Result<bool> {
    info!("Deleting {}", template);

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
                fs::remove_file(&template.cache).context("delete template cache")?;
                filesystem::delete_parents(&template.cache, false)
                    .context("delete parent directory in cache")?;
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
        TemplateComparison::Changed if !force => {
            error!(
                "Deleting {} but target contents were changed. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::Changed {
                warn!(
                    "Deleting {} but target contents were changed. Forcing.",
                    template
                );
            }

            debug!("Performing deletion");
            if act {
                fs::remove_file(&template.target.target).context("delete target file")?;
                filesystem::delete_parents(&template.target.target, true)
                    .context("delete parent directory in target location")?;
                fs::remove_file(&template.cache).context("delete cache file")?;
                filesystem::delete_parents(&template.cache, false)
                    .context("delete parent directory in cache")?;
            }
            Ok(true)
        }
    }
}

/// Returns true if symlink should be added to cache
fn create_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    info!("Creating {}", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
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
                std::fs::remove_file(&symlink.target)
                    .context("remove symlink target while forcing")?;
            }

            debug!("Performing creation");
            if act {
                fs::create_dir_all(
                    &symlink
                        .target
                        .parent()
                        .context("get parent of target file")?,
                )
                .context("create parent for target file")?;
                filesystem::make_symlink(&symlink.target, &symlink.source)
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
    info!("Creating {}", template);

    let comparison = filesystem::compare_template(&template.target.target, &template.cache)
        .context("detect templated file's current state")?;
    debug!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyCacheExists
        | TemplateComparison::Identical
        | TemplateComparison::Changed => {
            error!(
                "Creating {} but cache file already exists. Cache is CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        TemplateComparison::OnlyTargetExists if !force => {
            error!(
                "Creating {} but target file already exists. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::OnlyTargetExists {
                warn!(
                    "Creating {} but target file already exists. Forcing.",
                    template
                );
            }
            debug!("Performing creation");
            if act {
                perform_template_deployment(template, handlebars, variables)
                    .context("perform template deployment")?;
            }
            Ok(true)
        }
    }
}

// Returns true if the symlink wasn't skipped
fn update_symlink(act: bool, symlink: &SymlinkDescription, force: bool) -> Result<bool> {
    info!("Updating {}", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
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
        SymlinkComparison::Identical => {
            debug!("Not touching symlink.");
            Ok(true)
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!(
                    "Updating {} but target wasn't what was expected. Forcing.",
                    symlink
                );
                std::fs::remove_file(&symlink.target)
                    .context("remove symlink target while forcing")?;
            }
            if s == SymlinkComparison::OnlySourceExists {
                warn!(
                    "Updating {} but target was missing. Creating it anyways.",
                    symlink
                );
            }
            debug!("Creating missing symlink.");
            if act {
                fs::create_dir_all(
                    &symlink
                        .target
                        .parent()
                        .context("get parent of target file")?,
                )
                .context("create parent for target file")?;
                filesystem::make_symlink(&symlink.target, &symlink.source)
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
    handlebars: &Handlebars,
    variables: &Variables,
    force: bool,
) -> Result<bool> {
    info!("Updating {}", template);

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

            debug!("Performing update");
            if act {
                perform_template_deployment(template, handlebars, variables)
                    .context("perform template deployment")?;
            }
            Ok(true)
        }
    }
}

fn perform_template_deployment(
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
    fs::create_dir_all(
        &template
            .target
            .target
            .parent()
            .context("get parent of target file")?,
    )
    .context("create parent for target file")?;
    fs::copy(&template.cache, &template.target.target)
        .context("copy template from cache to target")?;
    filesystem::copy_permissions(&template.source, &template.target.target)
        .context("copy permissions from source to target")?;
    Ok(())
}

fn is_template(source: &Path) -> Result<bool> {
    let mut file = File::open(source).context("open file")?;
    let mut buf = String::new();
    if file.read_to_string(&mut buf).is_err() {
        warn!("File {:?} is not valid UTF-8 - not templating", source);
        Ok(false)
    } else {
        Ok(buf.contains("{{"))
    }
}

#[derive(Debug)]
struct FileState {
    desired_symlinks: BTreeSet<SymlinkDescription>,
    desired_templates: BTreeSet<TemplateDescription>,
    existing_symlinks: BTreeSet<SymlinkDescription>,
    existing_templates: BTreeSet<TemplateDescription>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SymlinkDescription {
    source: PathBuf,
    target: PathBuf,
}

impl std::cmp::PartialEq for TemplateDescription {
    fn eq(&self, other: &TemplateDescription) -> bool {
        self.source == other.source && self.target.target == other.target.target
    }
}
impl std::cmp::Eq for TemplateDescription {}

impl std::cmp::PartialOrd for TemplateDescription {
    fn partial_cmp(&self, other: &TemplateDescription) -> Option<std::cmp::Ordering> {
        Some(
            self.source
                .cmp(&other.source)
                .then(self.target.target.cmp(&other.target.target)),
        )
    }
}
impl std::cmp::Ord for TemplateDescription {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Clone)]
struct TemplateDescription {
    source: PathBuf,
    target: config::TemplateTarget,
    cache: PathBuf,
}

impl TemplateDescription {
    fn apply_actions(&self, mut file: String) -> String {
        if let Some(ref append) = self.target.append {
            file = file + append;
        }
        if let Some(ref prepend) = self.target.prepend {
            file = prepend.to_string() + &file;
        }

        file
    }
}

impl std::fmt::Display for SymlinkDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "symlink {:?} -> {:?}", self.source, self.target)
    }
}

impl std::fmt::Display for TemplateDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "template {:?} -> {:?}", self.source, self.target.target)
    }
}

impl FileState {
    fn new(
        desired_symlinks: BTreeMap<PathBuf, PathBuf>,
        desired_templates: BTreeMap<PathBuf, config::TemplateTarget>,
        existing_symlinks: BTreeMap<PathBuf, PathBuf>,
        existing_templates: BTreeMap<PathBuf, PathBuf>,
        cache_dir: PathBuf,
    ) -> FileState {
        FileState {
            desired_symlinks: Self::symlinks_to_set(desired_symlinks),
            desired_templates: Self::templates_to_set(desired_templates, &cache_dir),
            existing_symlinks: Self::symlinks_to_set(existing_symlinks),
            existing_templates: Self::templates_to_set(
                existing_templates
                    .into_iter()
                    .map(|(source, target)| {
                        (
                            source,
                            config::TemplateTarget {
                                target,
                                append: None,
                                prepend: None,
                            },
                        )
                    })
                    .collect(),
                &cache_dir,
            ),
        }
    }

    fn symlinks_to_set(symlinks: BTreeMap<PathBuf, PathBuf>) -> BTreeSet<SymlinkDescription> {
        symlinks
            .into_iter()
            .map(|(source, target)| SymlinkDescription { source, target })
            .collect()
    }

    fn templates_to_set(
        templates: BTreeMap<PathBuf, config::TemplateTarget>,
        cache_dir: &Path,
    ) -> BTreeSet<TemplateDescription> {
        templates
            .into_iter()
            .map(|(source, target)| TemplateDescription {
                source: source.clone(),
                target,
                cache: cache_dir.join(&source),
            })
            .collect()
    }

    fn deleted_files(&self) -> (Vec<SymlinkDescription>, Vec<TemplateDescription>) {
        (
            self.existing_symlinks
                .difference(&self.desired_symlinks)
                .cloned()
                .collect(),
            self.existing_templates
                .difference(&self.desired_templates)
                .cloned()
                .collect(),
        )
    }
    fn new_files(&self) -> (Vec<SymlinkDescription>, Vec<TemplateDescription>) {
        (
            self.desired_symlinks
                .difference(&self.existing_symlinks)
                .cloned()
                .collect(),
            self.desired_templates
                .difference(&self.existing_templates)
                .cloned()
                .collect(),
        )
    }
    fn old_files(&self) -> (Vec<SymlinkDescription>, Vec<TemplateDescription>) {
        (
            self.desired_symlinks
                .intersection(&self.existing_symlinks)
                .cloned()
                .collect(),
            self.desired_templates
                .intersection(&self.existing_templates)
                .cloned()
                .collect(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // TODO: test complex targets

    #[test]
    fn test_file_state_symlinks_only() {
        let mut existing_symlinks = BTreeMap::<PathBuf, PathBuf>::new();
        existing_symlinks.insert("file1s".into(), "file1t".into()); // Same
        existing_symlinks.insert("file2s".into(), "file2t".into()); // Deleted
        existing_symlinks.insert("file3s".into(), "file3t".into()); // Target change

        let mut desired_symlinks = BTreeMap::<PathBuf, PathBuf>::new();
        desired_symlinks.insert("file1s".into(), "file1t".into()); // Same
        desired_symlinks.insert("file3s".into(), "file0t".into()); // Target change
        desired_symlinks.insert("file5s".into(), "file5t".into()); // New

        let state = FileState::new(
            desired_symlinks,
            Default::default(),
            existing_symlinks,
            Default::default(),
            "cache".into(),
        );

        assert_eq!(
            state.deleted_files(),
            (
                vec![
                    SymlinkDescription {
                        source: "file2s".into(),
                        target: "file2t".into(),
                    },
                    SymlinkDescription {
                        source: "file3s".into(),
                        target: "file3t".into(),
                    }
                ],
                Vec::new()
            ),
            "deleted files correct"
        );
        assert_eq!(
            state.new_files(),
            (
                vec![
                    SymlinkDescription {
                        source: "file3s".into(),
                        target: "file0t".into(),
                    },
                    SymlinkDescription {
                        source: "file5s".into(),
                        target: "file5t".into(),
                    },
                ],
                Vec::new()
            ),
            "new files correct"
        );
        assert_eq!(
            state.old_files(),
            (
                vec![SymlinkDescription {
                    source: "file1s".into(),
                    target: "file1t".into(),
                }],
                Vec::new()
            ),
            "old files correct"
        );
    }

    #[test]
    fn test_file_state_complex() {}
}
