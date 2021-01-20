use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::config::{SymbolicTarget, TemplateTarget};
use crate::filesystem::{SymlinkComparison, TemplateComparison};

#[derive(Debug)]
enum Action {
    DeleteSymlink(CachedSymlink),
    DeleteTemplate(CachedTemplate),
    CreateSymlink(DesiredSymlink),
    CreateTemplate(DesiredTemplate),
    UpdateSymlink(DesiredSymlink),
    UpdateTemplate(DesiredTemplate),
}

#[derive(Debug)]
struct DesiredSymlink {
    source: PathBuf,
    target: SymbolicTarget,
    comparison: SymlinkComparison,
}

#[derive(Debug)]
struct DesiredTemplate {
    source: PathBuf,
    target: TemplateTarget,
    comparison: TemplateComparison,
}

#[derive(Debug)]
struct CachedSymlink {
    source: PathBuf,
    target: PathBuf,
    comparison: SymlinkComparison,
}

#[derive(Debug)]
struct CachedTemplate {
    source: PathBuf,
    target: PathBuf,
    comparison: TemplateComparison,
}

struct FileState {
    desired_symlinks: BTreeMap<PathBuf, DesiredSymlink>,
    desired_templates: BTreeMap<PathBuf, DesiredTemplate>,
    cached_symlinks: BTreeMap<PathBuf, CachedSymlink>,
    cached_templates: BTreeMap<PathBuf, CachedTemplate>,
}

fn plan_deploy(state: FileState) -> Vec<Action> {
    let mut actions = Vec::new();

    let FileState {
        mut desired_symlinks,
        mut desired_templates,
        mut cached_symlinks,
        mut cached_templates,
    } = state;

    let desired_symlinks_sources = desired_symlinks
        .keys()
        .cloned()
        .collect::<BTreeSet<PathBuf>>();
    let desired_templates_sources = desired_templates
        .keys()
        .cloned()
        .collect::<BTreeSet<PathBuf>>();
    let cached_symlinks_sources = cached_symlinks
        .keys()
        .cloned()
        .collect::<BTreeSet<PathBuf>>();
    let cached_templates_sources = cached_templates
        .keys()
        .cloned()
        .collect::<BTreeSet<PathBuf>>();

    for deleted_symlink in cached_symlinks_sources.difference(&desired_symlinks_sources) {
        actions.push(Action::DeleteSymlink(
            cached_symlinks.remove(deleted_symlink).unwrap(),
        ));
    }

    for deleted_template in cached_templates_sources.difference(&desired_templates_sources) {
        actions.push(Action::DeleteTemplate(
            cached_templates.remove(deleted_template).unwrap(),
        ));
    }

    for new_symlink in desired_symlinks_sources.difference(&cached_symlinks_sources) {
        actions.push(Action::CreateSymlink(
            desired_symlinks.remove(new_symlink).unwrap(),
        ));
    }

    for new_template in desired_templates_sources.difference(&cached_templates_sources) {
        actions.push(Action::CreateTemplate(
            desired_templates.remove(new_template).unwrap(),
        ));
    }

    for updated_symlink in desired_symlinks_sources.intersection(&cached_symlinks_sources) {
        actions.push(Action::UpdateSymlink(
            desired_symlinks.remove(updated_symlink).unwrap(),
        ));
    }

    for updated_template in desired_templates_sources.intersection(&cached_templates_sources) {
        actions.push(Action::UpdateTemplate(
            desired_templates.remove(updated_template).unwrap(),
        ));
    }

    actions
}
