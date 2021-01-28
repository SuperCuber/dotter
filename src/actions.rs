use anyhow::Result;

use std::path::PathBuf;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use crate::{
    args::Options,
    config::Cache,
    deploy::delete_symlink,
    file_state::{SymlinkDescription, TemplateDescription},
    filesystem::Filesystem,
};

#[derive(Debug, PartialEq, Eq)]
enum Action {
    DeleteSymlink(SymlinkDescription),
    DeleteTemplate(TemplateDescription),
    CreateSymlink(SymlinkDescription),
    CreateTemplate(TemplateDescription),
    UpdateSymlink(SymlinkDescription),
    UpdateTemplate(TemplateDescription),
}

#[derive(Debug)]
pub struct FileState {
    pub desired_symlinks: BTreeSet<SymlinkDescription>,
    pub desired_templates: BTreeSet<TemplateDescription>,
    pub existing_symlinks: BTreeSet<SymlinkDescription>,
    pub existing_templates: BTreeSet<TemplateDescription>,
}

fn plan_deploy(state: FileState) -> Vec<Action> {
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

impl Action {
    /// Returns true if action was successfully performed
    fn run(&self, fs: &mut impl Filesystem, opt: &Options) -> Result<bool> {
        match self {
            Action::DeleteSymlink(s) => delete_symlink(opt.act, &s, opt.force, opt.interactive),
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::config::{SymbolicTarget, TemplateTarget};

    use super::*;

    #[test]
    fn initial_deploy() {
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

        let actions = plan_deploy(file_state);
        assert_eq!(
            actions,
            [Action::CreateSymlink(a), Action::CreateTemplate(b)]
        );

        let mut fs = crate::filesystem::MockFilesystem::new();
        fs.expect_remove_file().times(1).returning(|_p| Ok(()));

        actions[0].run(&mut fs, &Options::default()).unwrap();
    }
}
