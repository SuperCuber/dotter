use anyhow::{Context, Result};

use handlebars::Handlebars;

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use args::Options;
use config::{self, Files, Variables};
use filesystem::{self, SymlinkComparison, TemplateComparison};
use handlebars_helpers;

pub fn undeploy(opt: Options) -> Result<()> {
    info!("Loading cache...");

    config::load_configuration(&opt.local_config, &opt.global_config)
        .context("Failed to find configuration location")?;

    let cache = match config::load_cache(&opt.cache_file)? {
        Some(cache) => cache,
        None => bail!("Failed to load cache: File not found"),
    };

    let config::Cache {
        symlinks: existing_symlinks,
        templates: existing_templates,
    } = cache;

    debug!("Existing symlinks: {:?}", existing_symlinks);
    debug!("Existing templates: {:?}", existing_templates);

    let state = FileState::new(
        Files::new(),
        Files::new(),
        existing_symlinks.clone(),
        existing_templates.clone(),
        opt.cache_directory,
    );
    debug!("File state: {:#?}", state);

    let (deleted_symlinks, deleted_templates) = state.deleted_files(); // Only those will exist

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
            Err(e) => display_error(e.context(format!("Failed to delete symlink {}", symlink))),
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
            Err(e) => display_error(e.context(format!("Failed to delete template {}", template))),
        }
    }

    if suggest_force {
        println!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
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
    // Configuration
    info!("Loading configuration...");

    // Throughout this function I'll be referencing steps, those were described in issue #6

    // Step 1
    let (files, variables, helpers) =
        config::load_configuration(&opt.local_config, &opt.global_config)
            .context("Failed to get a configuration.")?;

    // Step 2-3
    let mut desired_symlinks = config::Files::new();
    let mut desired_templates = config::Files::new();

    // On Windows, you need developer mode to create symlinks.
    let symlinks_enabled = if filesystem::symlinks_enabled(&PathBuf::from("DOTTER_SYMLINK_TEST"))
        .context("Failed to check whether symlinks are enabled")?
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

    for (source, target) in files {
        if symlinks_enabled
            && !is_template(&source).context(format!("check whether {:?} is a template", source))?
        {
            desired_symlinks.insert(source, target);
        } else {
            desired_templates.insert(source, target);
        }
    }

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
    debug!("File state: {:#?}", state);

    let mut actual_symlinks = existing_symlinks;
    let mut actual_templates = existing_templates;
    let mut suggest_force = false;

    // Step 5+6
    let (deleted_symlinks, deleted_templates) = state.deleted_files();
    debug!("Deleted symlinks: {:?}", deleted_symlinks);
    debug!("Deleted templates: {:?}", deleted_templates);
    for deleted_symlink in deleted_symlinks {
        match delete_symlink(opt.act, &deleted_symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.remove(&deleted_symlink.source);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("Failed to delete symlink {}", deleted_symlink)))
            }
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
            Err(e) => {
                display_error(e.context(format!("Failed to delete template {}", deleted_template)))
            }
        }
    }

    // Prepare handlebars instance
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, helpers);

    // Step 7+8
    let (new_symlinks, new_templates) = state.new_files();
    debug!("New symlinks: {:?}", new_symlinks);
    debug!("New templates: {:?}", new_templates);
    for new_symlink in new_symlinks {
        match create_symlink(opt.act, &new_symlink, opt.force) {
            Ok(true) => {
                actual_symlinks.insert(new_symlink.source, new_symlink.target);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => display_error(e.context(format!("Failed to create symlink {}", new_symlink))),
        }
    }
    for new_template in new_templates {
        match create_template(opt.act, &new_template, &handlebars, &variables, opt.force) {
            Ok(true) => {
                actual_templates.insert(new_template.source, new_template.target);
            }
            Ok(false) => {
                suggest_force = true;
            }
            Err(e) => {
                display_error(e.context(format!("Failed to create template {}", new_template)))
            }
        }
    }

    // Step 9+10
    let (old_symlinks, old_templates) = state.old_files();
    debug!("Old symlinks: {:?}", old_symlinks);
    debug!("Old templates: {:?}", old_templates);
    for old_symlink in old_symlinks {
        if let Err(e) = update_symlink(opt.act, &old_symlink, opt.force) {
            display_error(e.context(format!("Failed to update symlink {}", old_symlink)));
        }
    }
    for old_template in old_templates {
        if let Err(e) = update_template(opt.act, &old_template, &handlebars, &variables, opt.force)
        {
            display_error(e.context(format!("Failed to update template {}", old_template)));
        }
    }

    debug!("Actual symlinks: {:?}", actual_symlinks);
    debug!("Actual templates: {:?}", actual_templates);

    if suggest_force {
        println!("Some files were skipped. To ignore errors and overwrite unexpected target files, use the --force flag.");
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
fn delete_symlink(act: bool, symlink: &FileDescription, force: bool) -> Result<bool> {
    info!("Deleting symlink {}", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
        .context("Failed to detect symlink's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlySourceExists | SymlinkComparison::BothMissing => {
            warn!(
                "Deleting symlink {} but target doesn't exist. Removing from cache anyways.",
                symlink
            );
            Ok(true)
        }
        SymlinkComparison::Changed if !force => {
            error!(
                "Deleting symlink {} but target doesn't point at source file. Skipping...",
                symlink
            );
            Ok(false)
        }
        SymlinkComparison::TargetNotSymlink if !force => {
            error!(
                "Deleting symlink {} but target isn't a symlink. Skipping...",
                symlink
            );
            Ok(false)
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!(
                    "Deleting symlink {} but target wasn't what was expected. Forcing.",
                    symlink
                );
            }

            info!("Performing deletion");
            if act {
                fs::remove_file(&symlink.target).context("Failed to remove symlink")?;
                filesystem::delete_parents(&symlink.target, true)
                    .context("Failed to delete parents of symlink")?;
            }
            Ok(true)
        }
    }
}

/// Returns true if template should be deleted from cache
fn delete_template(act: bool, template: &FileDescription, force: bool) -> Result<bool> {
    info!("Deleting template {}", template);

    let comparison = filesystem::compare_template(&template.target, &template.cache)
        .context("Failed to detect templated file's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyCacheExists => {
            warn!(
                "Deleting template {} but target doesn't exist. Deleting cache anyways.",
                template
            );
            if act {
                fs::remove_file(&template.cache).context("Failed delete template cache")?;
                filesystem::delete_parents(&template.cache, false)
                    .context("Failed to delete parent directory in cache")?;
            }
            Ok(true)
        }
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Deleting template {} but cache doesn't exist. Cache probably CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        TemplateComparison::Changed if !force => {
            error!(
                "Deleting template {} but target contents were changed. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::Changed {
                warn!(
                    "Deleting template {} but target contents were changed. Forcing.",
                    template
                );
            }

            info!("Performing deletion");
            if act {
                fs::remove_file(&template.target).context("Failed to remove target file")?;
                filesystem::delete_parents(&template.target, true)
                    .context("Failed to delete parent directory in target location")?;
                fs::remove_file(&template.cache).context("Failed to remove cache file")?;
                filesystem::delete_parents(&template.cache, false)
                    .context("Failed to delete parent directory in cache")?;
            }
            Ok(true)
        }
    }
}

/// Returns true if symlink should be added to cache
fn create_symlink(act: bool, symlink: &FileDescription, force: bool) -> Result<bool> {
    info!("Creating symlink {}", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
        .context("Failed to detect symlink's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!(
                "Creating symlink {} but source is missing. Skipping...",
                symlink
            );
            Ok(false)
        }
        SymlinkComparison::Identical => {
            warn!("Creating symlink {} but target already exists and points at source. Adding to cache anyways", symlink);
            Ok(true)
        }
        SymlinkComparison::Changed | SymlinkComparison::TargetNotSymlink if !force => {
            error!("Creating symlink {} but target already exists and differs from expected. Skipping...", symlink);
            Ok(false)
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!("Creating symlink {} but target already exists and differs from expected. Forcing.", symlink);
                info!("Force deleting target {:?}", symlink.target);
                std::fs::remove_file(&symlink.target).with_context(|| {
                    format!(
                        "Failed to remove symlink target {:?} while forcing",
                        symlink.target
                    )
                })?;
            }

            info!("Performing creation");
            if act {
                fs::create_dir_all(
                    &symlink
                        .target
                        .parent()
                        .context("Failed to get parent of target file")?,
                )
                .context("Failed to create parent for target file")?;
                filesystem::make_symlink(&symlink.target, &symlink.source)
                    .context("Failed to create target symlink")?;
            }
            Ok(true)
        }
    }
}

fn create_template(
    act: bool,
    template: &FileDescription,
    handlebars: &Handlebars,
    variables: &Variables,
    force: bool,
) -> Result<bool> {
    info!("Creating template {}", template);

    let comparison = filesystem::compare_template(&template.target, &template.cache)
        .context("Failed to detect templated file's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyCacheExists
        | TemplateComparison::Identical
        | TemplateComparison::Changed => {
            error!(
                "Creating template {} but cache file already exists. Cache is CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
            Ok(false)
        }
        TemplateComparison::OnlyTargetExists if !force => {
            error!(
                "Creating template {} but target file already exists. Skipping...",
                template
            );
            Ok(false)
        }
        t => {
            if t == TemplateComparison::OnlyTargetExists {
                warn!(
                    "Creating template {} but target file already exists. Forcing.",
                    template
                );
            }
            info!("Performing creation");
            if act {
                perform_template_deployment(template, handlebars, variables)
                    .context("Failed to perform template deployment")?;
            }
            Ok(true)
        }
    }
}

fn update_symlink(act: bool, symlink: &FileDescription, force: bool) -> Result<()> {
    info!("Updating symlink {}", symlink);

    let comparison = filesystem::compare_symlink(&symlink.source, &symlink.target)
        .context("Failed to detect symlink's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        SymlinkComparison::OnlyTargetExists | SymlinkComparison::BothMissing => {
            error!(
                "Updating symlink {} but source is missing. Skipping...",
                symlink
            );
        }
        SymlinkComparison::Changed if !force => {
            error!(
                "Updating symlink {} but target doesn't point at source. Skipping...",
                symlink
            );
        }
        SymlinkComparison::TargetNotSymlink if !force => {
            error!(
                "Updating symlink {} but target is not a symlink. Skipping...",
                symlink
            );
        }
        SymlinkComparison::Identical => {
            info!("Not touching symlink.");
        }
        s => {
            if s == SymlinkComparison::Changed || s == SymlinkComparison::TargetNotSymlink {
                warn!(
                    "Updating symlink {} but target wasn't what was expected. Forcing.",
                    symlink
                );
                info!("Force deleting target {:?}", symlink.target);
                std::fs::remove_file(&symlink.target).with_context(|| {
                    format!(
                        "Failed to remove symlink target {:?} while forcing",
                        symlink.target
                    )
                })?;
            }
            if s == SymlinkComparison::OnlySourceExists {
                warn!(
                    "Updating symlink {} but target was missing. Creating it anyways.",
                    symlink
                );
            }
            info!("Creating missing symlink.");
            if act {
                fs::create_dir_all(
                    &symlink
                        .target
                        .parent()
                        .context("Failed to get parent of target file")?,
                )
                .context("Failed to create parent for target file")?;
                filesystem::make_symlink(&symlink.target, &symlink.source)
                    .context("Failed to create target symlink")?;
            }
        }
    }
    Ok(())
}

fn update_template(
    act: bool,
    template: &FileDescription,
    handlebars: &Handlebars,
    variables: &Variables,
    force: bool,
) -> Result<()> {
    info!("Updating template {}", template);

    let comparison = filesystem::compare_template(&template.target, &template.cache)
        .context("Failed to detect templated file's current state")?;
    info!("Current state: {}", comparison);

    match comparison {
        TemplateComparison::OnlyTargetExists | TemplateComparison::BothMissing => {
            error!(
                "Updating template {} but cache is missing. Cache is CORRUPTED.",
                template
            );
            error!("This is probably a bug. Delete cache.toml and cache/ folder.");
        }
        TemplateComparison::Changed if !force => {
            error!(
                "Updating template {} but target's contents were changed. Skipping...",
                template
            );
        }
        t => {
            if t == TemplateComparison::Changed {
                warn!(
                    "Updating template {} but target's contents were changed. Forcing.",
                    template
                );
            }

            info!("Performing update");
            if act {
                perform_template_deployment(template, handlebars, variables)
                    .context("Failed to perform template deployment")?;
            }
        }
    }

    Ok(())
}

fn perform_template_deployment(
    template: &FileDescription,
    handlebars: &Handlebars,
    variables: &Variables,
) -> Result<()> {
    let rendered = handlebars
        .render_template(
            &fs::read_to_string(&template.source).context("Failed to read template source file")?,
            variables,
        )
        .context("Failed to render template")?;
    fs::create_dir_all(
        &template
            .cache
            .parent()
            .context("Failed to get parent of cache file")?,
    )
    .context("Failed to create parent for cache file")?;
    fs::write(&template.cache, rendered).context("Failed to write rendered template to cache")?;
    fs::create_dir_all(
        &template
            .target
            .parent()
            .context("Failed to get parent of target file")?,
    )
    .context("Failed to create parent for target file")?;
    fs::copy(&template.cache, &template.target)
        .context("Failed to copy template from cache to target")?;
    filesystem::copy_permissions(&template.source, &template.target)
        .context("Failed to copy permissions from source to target")?;
    Ok(())
}

fn is_template(source: &Path) -> Result<bool> {
    let mut file = File::open(source).context(format!("Failed to open file {:?}", source))?;
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
    desired_symlinks: BTreeSet<FileDescription>,
    desired_templates: BTreeSet<FileDescription>,
    existing_symlinks: BTreeSet<FileDescription>,
    existing_templates: BTreeSet<FileDescription>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
struct FileDescription {
    source: PathBuf,
    target: PathBuf,
    cache: PathBuf,
}

impl std::fmt::Display for FileDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?} -> {:?}", self.source, self.target)
    }
}

impl FileState {
    fn new(
        desired_symlinks: Files,
        desired_templates: Files,
        existing_symlinks: Files,
        existing_templates: Files,
        cache_dir: PathBuf,
    ) -> FileState {
        FileState {
            desired_symlinks: Self::files_to_set(desired_symlinks, &cache_dir),
            desired_templates: Self::files_to_set(desired_templates, &cache_dir),
            existing_symlinks: Self::files_to_set(existing_symlinks, &cache_dir),
            existing_templates: Self::files_to_set(existing_templates, &cache_dir),
        }
    }

    fn files_to_set(files: Files, cache_dir: &Path) -> BTreeSet<FileDescription> {
        files
            .into_iter()
            .map(|(source, target)| FileDescription {
                source: source.clone(),
                target,
                cache: cache_dir.join(source),
            })
            .collect()
    }

    fn deleted_files(&self) -> (Vec<FileDescription>, Vec<FileDescription>) {
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
    fn new_files(&self) -> (Vec<FileDescription>, Vec<FileDescription>) {
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
    fn old_files(&self) -> (Vec<FileDescription>, Vec<FileDescription>) {
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

fn display_error(error: anyhow::Error) {
    let mut chain = error.chain();
    let mut error_message = format!("{}\nCaused by:\n", chain.next().unwrap());

    for e in chain {
        error_message.push_str(&format!("    {}\n", e));
    }
    // Remove last \n
    error_message.pop();

    error!("{}", error_message);
}

#[cfg(test)]
mod test {
    use super::{FileDescription, FileState, Files, PathBuf};

    #[test]
    fn test_file_state_symlinks_only() {
        // Testing symlinks only is enough for me because the logic should be the same
        let mut existing_symlinks = Files::new();
        existing_symlinks.insert("file1s".into(), "file1t".into()); // Same
        existing_symlinks.insert("file2s".into(), "file2t".into()); // Deleted
        existing_symlinks.insert("file3s".into(), "file3t".into()); // Target change

        let mut desired_symlinks = Files::new();
        desired_symlinks.insert("file1s".into(), "file1t".into()); // Same
        desired_symlinks.insert("file3s".into(), "file0t".into()); // Target change
        desired_symlinks.insert("file5s".into(), "file5t".into()); // New

        let state = FileState::new(
            desired_symlinks,
            Files::new(),
            existing_symlinks,
            Files::new(),
            "cache".into(),
        );

        assert_eq!(
            state.deleted_files(),
            (
                vec![
                    FileDescription {
                        source: "file2s".into(),
                        target: "file2t".into(),
                        cache: PathBuf::from("cache").join("file2s"),
                    },
                    FileDescription {
                        source: "file3s".into(),
                        target: "file3t".into(),
                        cache: PathBuf::from("cache").join("file3s"),
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
                    FileDescription {
                        source: "file3s".into(),
                        target: "file0t".into(),
                        cache: PathBuf::from("cache").join("file3s")
                    },
                    FileDescription {
                        source: "file5s".into(),
                        target: "file5t".into(),
                        cache: PathBuf::from("cache").join("file5s")
                    },
                ],
                Vec::new()
            ),
            "new files correct"
        );
        assert_eq!(
            state.old_files(),
            (
                vec![FileDescription {
                    source: "file1s".into(),
                    target: "file1t".into(),
                    cache: PathBuf::from("cache").join("file1s"),
                }],
                Vec::new()
            ),
            "old files correct"
        );
    }
}
