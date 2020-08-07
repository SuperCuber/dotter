use handlebars::{Handlebars, TemplateRenderError};

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process;

use args::Options;
use config::{self, FilesPath, Variables};
use handlebars_helpers;

pub fn deploy(opt: Options) {
    // Configuration
    info!("Loading configuration...");

    // Throughout this function I'll be referencing steps, those were described in issue #6

    // Step 1
    let (files, variables, helpers) =
        config::load_configuration(&opt.local_config, &opt.global_config).unwrap_or_else(|| {
            error!("Failed to find configuration in current or parent directories.");
            process::exit(1);
        });

    // Step 2-3
    let mut desired_symlinks = config::FilesPath::new();
    let mut desired_templates = config::FilesPath::new();

    for (source, target) in files {
        if is_template(&source) {
            desired_templates.insert(source, target);
        } else {
            desired_symlinks.insert(source, target);
        }
    }

    // Step 4
    let (existing_symlinks, existing_templates) = config::load_cache(&opt.cache_file);

    let state = FileState::new(
        desired_symlinks,
        desired_templates,
        existing_symlinks,
        existing_templates,
        opt.cache_directory,
    );

    // Step 5+6
    let (deleted_symlinks, deleted_templates) = state.deleted_files();
    for deleted_symlink in deleted_symlinks {
        delete_symlink(opt.act, deleted_symlink);
    }
    for deleted_template in deleted_templates {
        delete_template(opt.act, deleted_template);
    }

    // Prepare handlebars instance
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string()); // Disable html-escaping
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, helpers);

    // Step 7+8
    let (new_symlinks, new_templates) = state.new_files();
    for new_symlink in new_symlinks {
        create_symlink(opt.act, new_symlink);
    }
    for new_template in new_templates {
        create_template(opt.act, new_template, &handlebars, &variables);
    }

    // Step 9+10
    let (old_symlinks, old_templates) = state.old_files();
    for old_symlink in old_symlinks {
        update_symlink(opt.act, old_symlink);
    }
    for old_template in old_templates {
        update_template(opt.act, old_template, &handlebars, &variables);
    }
}

fn delete_symlink(act: bool, new_symlink : FileDescription) { }
fn delete_template(act: bool, new_symlink : FileDescription) { }
fn create_symlink(act: bool, new_symlink : FileDescription) { }
fn create_template(act: bool, new_symlink : FileDescription, handlebars: &Handlebars, variables: &Variables) { }
fn update_symlink(act: bool, new_symlink : FileDescription) { }
fn update_template(act: bool, new_symlink: FileDescription, handlebars: &Handlebars, variables: &Variables) { }

fn is_template(source: &Path) -> bool {
    let mut file = File::open(source).unwrap_or_else(|e| {
        error!("Failed to open file {:?} because {}", source, e);
        process::exit(1);
    });
    let mut buf = String::new();
    if let Err(_) = file.read_to_string(&mut buf) {
        warn!("File {:?} is not valid UTF-8 - not templating", source);
        false
    } else {
        buf.contains("{{")
    }
}

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

impl FileState {
    fn new(
        desired_symlinks: FilesPath,
        desired_templates: FilesPath,
        existing_symlinks: FilesPath,
        existing_templates: FilesPath,
        cache_dir: PathBuf,
    ) -> FileState {
        FileState {
            desired_symlinks: Self::files_to_set(desired_symlinks, &cache_dir),
            desired_templates: Self::files_to_set(desired_templates, &cache_dir),
            existing_symlinks: Self::files_to_set(existing_symlinks, &cache_dir),
            existing_templates: Self::files_to_set(existing_templates, &cache_dir),
        }
    }

    fn files_to_set(files: FilesPath, cache_dir: &Path) -> BTreeSet<FileDescription> {
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
            self.existing_templates
                .intersection(&self.existing_templates)
                .cloned()
                .collect(),
        )
    }
    fn serialize(&self) -> (config::Files, config::Files) {
        todo!()
    } // Step 11
}

#[cfg(test)]
mod test {
    use super::{FileDescription, FileState, FilesPath, PathBuf};

    #[test]
    fn test_file_state_symlinks_only() {
        // Testing symlinks only is enough for me because the logic should be the same
        let mut existing_symlinks = FilesPath::new();
        existing_symlinks.insert("file1s".into(), "file1t".into()); // Same
        existing_symlinks.insert("file2s".into(), "file2t".into()); // Deleted
        existing_symlinks.insert("file3s".into(), "file3t".into()); // Target change

        let mut desired_symlinks = FilesPath::new();
        desired_symlinks.insert("file1s".into(), "file1t".into()); // Same
        desired_symlinks.insert("file3s".into(), "file0t".into()); // Target change
        desired_symlinks.insert("file5s".into(), "file5t".into()); // New

        let state = FileState::new(
            desired_symlinks,
            FilesPath::new(),
            existing_symlinks,
            FilesPath::new(),
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
