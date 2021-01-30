use anyhow::{Context, Result};

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config;
use crate::filesystem;

#[derive(Debug)]
pub struct FileState {
    pub desired_symlinks: BTreeSet<SymlinkDescription>,
    pub desired_templates: BTreeSet<TemplateDescription>,
    pub existing_symlinks: BTreeSet<SymlinkDescription>,
    pub existing_templates: BTreeSet<TemplateDescription>,
}

#[derive(Debug, Clone)]
pub struct SymlinkDescription {
    pub source: PathBuf,
    pub target: config::SymbolicTarget,
}

#[derive(Debug, Clone)]
pub struct TemplateDescription {
    pub source: PathBuf,
    pub target: config::TemplateTarget,
    pub cache: PathBuf,
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
            // TODO: transpose `if symlinks_enabled` and `match`
            config::FileTarget::Automatic(target) => {
                if symlinks_enabled
                    && !is_template(&source)
                        .context(format!("check whether {:?} is a template", source))?
                {
                    desired_symlinks.insert(source, target.into());
                } else {
                    desired_templates.insert(source, target.into());
                }
            }
            config::FileTarget::Symbolic(target) => {
                if symlinks_enabled {
                    desired_symlinks.insert(source, target);
                } else {
                    desired_templates.insert(source, target.into_template());
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

// For use in FileState's Sets
impl std::cmp::PartialEq for SymlinkDescription {
    fn eq(&self, other: &SymlinkDescription) -> bool {
        self.source == other.source && self.target.target == other.target.target
    }
}
impl std::cmp::Eq for SymlinkDescription {}
impl std::cmp::PartialOrd for SymlinkDescription {
    fn partial_cmp(&self, other: &SymlinkDescription) -> Option<std::cmp::Ordering> {
        Some(
            self.source
                .cmp(&other.source)
                .then(self.target.target.cmp(&other.target.target)),
        )
    }
}
impl std::cmp::Ord for SymlinkDescription {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
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

impl TemplateDescription {
    pub fn apply_actions(&self, mut file: String) -> String {
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
        write!(f, "symlink {:?} -> {:?}", self.source, self.target.target)
    }
}

impl std::fmt::Display for TemplateDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "template {:?} -> {:?}", self.source, self.target.target)
    }
}

impl FileState {
    pub fn new(
        desired_symlinks: BTreeMap<PathBuf, config::SymbolicTarget>,
        desired_templates: BTreeMap<PathBuf, config::TemplateTarget>,
        existing_symlinks: BTreeMap<PathBuf, PathBuf>,
        existing_templates: BTreeMap<PathBuf, PathBuf>,
        cache_dir: impl AsRef<Path>,
    ) -> FileState {
        FileState {
            desired_symlinks: Self::symlinks_to_set(desired_symlinks),
            desired_templates: Self::templates_to_set(desired_templates, cache_dir.as_ref()),
            existing_symlinks: Self::symlinks_to_set(
                existing_symlinks
                    .into_iter()
                    .map(|(source, target)| {
                        (
                            source,
                            config::SymbolicTarget {
                                target,
                                owner: None,
                            },
                        )
                    })
                    .collect(),
            ),
            existing_templates: Self::templates_to_set(
                existing_templates
                    .into_iter()
                    .map(|(source, target)| {
                        (
                            source,
                            config::TemplateTarget {
                                target,
                                owner: None,
                                append: None,
                                prepend: None,
                            },
                        )
                    })
                    .collect(),
                cache_dir.as_ref(),
            ),
        }
    }

    pub fn symlinks_to_set(
        symlinks: BTreeMap<PathBuf, config::SymbolicTarget>,
    ) -> BTreeSet<SymlinkDescription> {
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
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_file_state_symlinks_only() {
        let mut existing_symlinks = BTreeMap::new();
        existing_symlinks.insert("file1s".into(), "file1t".into()); // Same
        existing_symlinks.insert("file2s".into(), "file2t".into()); // Deleted
        existing_symlinks.insert("file3s".into(), "file3t".into()); // Target change

        let mut desired_symlinks = BTreeMap::new();
        desired_symlinks.insert("file1s".into(), "file1t".into()); // Same
        desired_symlinks.insert("file3s".into(), "file0t".into()); // Target change
        desired_symlinks.insert("file5s".into(), "file5t".into()); // New

        let state = FileState::new(
            desired_symlinks,
            Default::default(),
            existing_symlinks,
            Default::default(),
            "cache",
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
}
