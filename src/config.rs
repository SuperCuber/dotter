use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};

use crate::filesystem;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum UnixUser {
    Uid(i32),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SymbolicTarget {
    pub target: PathBuf,
    pub owner: Option<UnixUser>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct TemplateTarget {
    pub target: PathBuf,
    pub owner: Option<UnixUser>,
    pub append: Option<String>,
    pub prepend: Option<String>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
}

// Deserialize implemented manually
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileTarget {
    Automatic(PathBuf),
    Symbolic(SymbolicTarget),
    ComplexTemplate(TemplateTarget),
}

pub type Files = BTreeMap<PathBuf, FileTarget>;
pub type Variables = toml::value::Table;
pub type Helpers = BTreeMap<String, PathBuf>;

#[derive(Debug, Clone)]
pub struct Configuration {
    pub files: Files,
    pub variables: Variables,
    pub helpers: Helpers,
    pub packages: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Package {
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    files: Files,
    #[serde(default)]
    variables: Variables,
}

#[derive(Debug, Deserialize, Serialize)]
struct GlobalConfig {
    #[serde(default)]
    helpers: Helpers,
    #[serde(flatten)]
    packages: BTreeMap<String, Package>,
}

type IncludedConfig = BTreeMap<String, Package>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocalConfig {
    #[serde(default)]
    includes: Vec<PathBuf>,
    packages: Vec<String>,
    #[serde(default)]
    files: Files,
    #[serde(default)]
    variables: Variables,
}

pub fn load_configuration(
    local_config: &Path,
    global_config: &Path,
    patch: Option<Package>,
) -> Result<Configuration> {
    let global: GlobalConfig = filesystem::load_file(global_config)
        .and_then(|c| c.ok_or_else(|| anyhow::anyhow!("file not found")))
        .with_context(|| format!("load global config {:?}", global_config))?;
    trace!("Global config: {:#?}", global);

    // If local.toml can't be found, look for a file named <hostname>.toml instead
    let mut local_config_buf = local_config.to_path_buf();
    if !local_config_buf.exists() {
        let hostname = hostname::get()
            .context("failed to get the computer hostname")?
            .into_string()
            .expect("hostname cannot be converted to string");
        info!(
            "{:?} not found, using {}.toml instead (based on hostname)",
            local_config, hostname
        );
        local_config_buf.set_file_name(&format!("{}.toml", hostname));
    }

    let local: LocalConfig = filesystem::load_file(local_config_buf.as_path())
        .and_then(|c| c.ok_or_else(|| anyhow::anyhow!("file not found")))
        .with_context(|| format!("load local config {:?}", local_config))?;
    trace!("Local config: {:#?}", local);

    let mut merged_config =
        merge_configuration_files(global, local, patch).context("merge configuration files")?;
    trace!("Merged config: {:#?}", merged_config);

    debug!("Expanding files which are directories...");
    merged_config.files =
        expand_directories(merged_config.files).context("expand files that are directories")?;

    debug!("Expanding tildes to home directory...");
    merged_config.files = merged_config
        .files
        .into_iter()
        .map(|(k, mut v)| -> Result<_, anyhow::Error> {
            let path = v.path();
            let path = shellexpand::full(&path.to_string_lossy())
                .context("failed to expand file path")?
                .to_string();
            v.set_path(path);
            Ok((k, v))
        })
        .collect::<Result<_, _>>()?;

    trace!("Final files: {:#?}", merged_config.files);
    trace!("Final variables: {:#?}", merged_config.variables);
    trace!("Final helpers: {:?}", merged_config.helpers);

    Ok(merged_config)
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    pub symlinks: BTreeMap<PathBuf, PathBuf>,
    pub templates: BTreeMap<PathBuf, PathBuf>,
}

pub fn save_dummy_config(
    files: Vec<String>,
    local_config_path: &Path,
    global_config_path: &Path,
) -> Result<()> {
    debug!("Saving dummy config...");
    let package = Package {
        files: files.into_iter().map(|f| (f.into(), "".into())).collect(),
        variables: Variables::new(),
        depends: vec![],
    };
    trace!("Default package: {:#?}", package);

    let mut packages = BTreeMap::new();
    packages.insert("default".into(), package);
    let global_config = GlobalConfig {
        helpers: Helpers::new(),
        packages,
    };
    debug!("Saving global config...");
    // Assume default args so all parents are the same
    std::fs::create_dir_all(
        global_config_path
            .parent()
            .context("get parent of global config")?,
    )
    .context("create parent of global config")?;
    filesystem::save_file(global_config_path, global_config).context("save global config")?;

    let local_config = LocalConfig {
        includes: vec![],
        packages: vec!["default".into()],
        files: Files::default(),
        variables: Variables::default(),
    };
    trace!("Local config: {:#?}", local_config);
    filesystem::save_file(local_config_path, local_config).context("save local config")?;

    Ok(())
}

fn recursive_extend_map(
    original: &mut BTreeMap<String, toml::Value>,
    new: BTreeMap<String, toml::Value>,
) {
    for (key, new_value) in new {
        original
            .entry(key)
            .and_modify(|original_value| {
                match (
                    original_value.as_table().cloned(),
                    new_value.as_table().cloned(),
                ) {
                    (Some(mut original_table), Some(new_table)) => {
                        recursive_extend_map(&mut original_table, new_table);
                        *original_value = original_table.into();
                    }
                    _ => *original_value = new_value.clone(),
                }
            })
            .or_insert(new_value);
    }
}

#[allow(clippy::map_entry)]
fn merge_configuration_files(
    mut global: GlobalConfig,
    local: LocalConfig,
    patch: Option<Package>,
) -> Result<Configuration> {
    // Patch each package with included.toml's
    for included_path in &local.includes {
        || -> Result<()> {
            let mut included: IncludedConfig = filesystem::load_file(&included_path)
                .and_then(|c| c.ok_or_else(|| anyhow::anyhow!("file not found")))
                .context("load file")?;

            debug!("Included config {:?}", included_path);
            trace!("{:#?}", included);

            // If package isn't filtered it's ignored, if package isn't included it's ignored
            for (package_name, package_global) in &mut global.packages {
                if let Some(package_included) = included.remove(package_name) {
                    package_global.files.extend(package_included.files);
                    recursive_extend_map(&mut package_global.variables, package_included.variables);
                }
            }

            if !included.is_empty() {
                anyhow::bail!(
                    "unknown packages: {:?}",
                    included.keys().into_iter().cloned().collect::<Vec<_>>()
                );
            }

            Ok(())
        }()
        .with_context(|| format!("including file {:?}", included_path))?;
    }

    // Enable depended packages
    let mut enabled_packages = local.packages.clone().into_iter().collect::<BTreeSet<_>>();
    let mut package_count = 0;

    // Keep iterating until there's nothing new added
    while enabled_packages.len() > package_count {
        let mut new_packages = BTreeSet::new();
        for package in &enabled_packages {
            new_packages.extend(
                global
                    .packages
                    .get(package)
                    .with_context(|| format!("get info of package {}", package))?
                    .depends
                    .clone(),
            );
        }
        package_count = enabled_packages.len();
        enabled_packages.extend(new_packages);
    }

    // Apply packages filter
    global.packages = global
        .packages
        .into_iter()
        .filter(|(k, _)| enabled_packages.contains(k))
        .collect();

    let mut output = Configuration {
        helpers: global.helpers,
        files: Files::default(),
        variables: Variables::default(),
        packages: enabled_packages.into_iter().collect(),
    };

    // Merge all the packages
    let mut configuration_packages = global.packages.into_iter();
    let mut first_package = configuration_packages
        .next()
        .unwrap_or_else(|| (String::new(), Package::default()))
        .1;
    for (package_name, package) in configuration_packages {
        || -> Result<()> {
            for (file_name, file_target) in package.files {
                if first_package.files.contains_key(&file_name) {
                    anyhow::bail!("file {:?} already encountered", file_name);
                } else {
                    first_package.files.insert(file_name, file_target);
                }
            }

            for (variable_name, variable_value) in package.variables {
                if first_package.variables.contains_key(&variable_name) {
                    anyhow::bail!("variable {:?} already encountered", variable_name);
                } else {
                    first_package
                        .variables
                        .insert(variable_name, variable_value);
                }
            }

            Ok(())
        }()
        .with_context(|| format!("merge package {:?}", package_name))?;
    }
    output.files = first_package.files;
    output.variables = first_package.variables;

    // Add local.toml's patches
    output.files.extend(local.files);
    recursive_extend_map(&mut output.variables, local.variables);

    // Add manual patch
    if let Some(patch) = patch {
        output.files.extend(patch.files);
        recursive_extend_map(&mut output.variables, patch.variables);
    }

    // Remove files with target = ""
    output.files = output
        .files
        .into_iter()
        .filter(|(_, v)| v.path().to_string_lossy() != "")
        .collect();

    Ok(output)
}

impl<'de> Deserialize<'de> for FileTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(remote = "FileTarget", tag = "type", rename_all = "snake_case")]
        enum This {
            Automatic(PathBuf),
            Symbolic(SymbolicTarget),
            #[serde(rename = "template")]
            ComplexTemplate(TemplateTarget),
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Simple(PathBuf),
            #[serde(with = "This")]
            Complex(FileTarget),
        }

        Ok(match Helper::deserialize(deserializer)? {
            Helper::Simple(target) => Self::Automatic(target),
            Helper::Complex(this) => this,
        })
    }
}

impl FileTarget {
    pub fn path(&self) -> &Path {
        match self {
            FileTarget::Automatic(path) => &path,
            FileTarget::Symbolic(SymbolicTarget { target, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { target, .. }) => &target,
        }
    }

    pub fn set_path(&mut self, new_path: impl Into<PathBuf>) {
        match self {
            FileTarget::Automatic(ref mut path) => *path = new_path.into(),
            FileTarget::Symbolic(SymbolicTarget { target, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { target, .. }) => {
                *target = new_path.into()
            }
        }
    }

    pub fn condition(&self) -> Option<&String> {
        match self {
            FileTarget::Automatic(_) => None,
            FileTarget::Symbolic(SymbolicTarget { condition, .. }) => condition.as_ref(),
            FileTarget::ComplexTemplate(TemplateTarget { condition, .. }) => condition.as_ref(),
        }
    }
}

impl<T: Into<PathBuf>> From<T> for FileTarget {
    fn from(input: T) -> Self {
        FileTarget::Automatic(input.into())
    }
}

impl<T: Into<PathBuf>> From<T> for SymbolicTarget {
    fn from(input: T) -> Self {
        SymbolicTarget {
            target: input.into(),
            owner: None,
            condition: None,
        }
    }
}

impl<T: Into<PathBuf>> From<T> for TemplateTarget {
    fn from(input: T) -> Self {
        TemplateTarget {
            target: input.into(),
            owner: None,
            append: None,
            prepend: None,
            condition: None,
        }
    }
}

impl SymbolicTarget {
    pub fn into_template(self) -> TemplateTarget {
        TemplateTarget {
            target: self.target,
            owner: self.owner,
            condition: self.condition,
            prepend: None,
            append: None,
        }
    }
}

impl TemplateTarget {
    pub fn apply_actions(&self, mut file: String) -> String {
        if let Some(ref append) = self.append {
            file += append.as_str();
        }
        if let Some(ref prepend) = self.prepend {
            file = prepend.to_string() + file.as_str();
        }

        file
    }
}

fn expand_directories(files: Files) -> Result<Files> {
    let expanded = files
        .into_iter()
        .map(|(from, to)| expand_directory(&from, to).context(format!("expand file {:?}", from)))
        .collect::<Result<Vec<Files>>>()?;
    Ok(expanded.into_iter().flatten().collect::<Files>())
}

/// If a file is given, it will return a map of one element
/// Otherwise, returns recursively all the children and their targets
///  in relation to parent target
fn expand_directory(source: &Path, target: FileTarget) -> Result<Files> {
    if fs::metadata(source)
        .context("read file's metadata")?
        .is_file()
    {
        let mut map = Files::new();
        map.insert(source.into(), target);
        Ok(map)
    } else {
        let expanded = fs::read_dir(source)
            .context("read contents of directory")?
            .map(|child| -> Result<Files> {
                let child = child?.file_name();
                let child_source = PathBuf::from(source).join(&child);
                let mut child_target = target.clone();
                child_target.set_path(child_target.path().join(&child));
                expand_directory(&child_source, child_target)
                    .context(format!("expand file {:?}", child_source))
            })
            .collect::<Result<Vec<Files>>>()?; // Use transposition of Iterator<Result<T,E>> -> Result<Sequence<T>, E>
        Ok(expanded.into_iter().flatten().collect())
    }
}

#[cfg(unix)]
impl UnixUser {
    pub fn as_sudo_arg(&self) -> String {
        match self {
            UnixUser::Name(n) => n.clone(),
            UnixUser::Uid(id) => format!("#{}", id),
        }
    }

    pub fn as_chown_arg(&self) -> String {
        match self {
            UnixUser::Name(n) => n.clone(),
            UnixUser::Uid(id) => format!("{}", id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_file_target() {
        #[derive(Deserialize)]
        struct Helper {
            file: FileTarget,
        }

        let parse = |s| toml::from_str::<Helper>(s);

        assert_eq!(
            parse(
                r#"
                    file = '~/.QuarticCat'
                "#,
            )
            .unwrap()
            .file,
            FileTarget::Automatic(PathBuf::from("~/.QuarticCat")),
        );
        assert_eq!(
            parse(
                r#"
                    [file]
                    target = '~/.QuarticCat'
                    type = 'symbolic'
                "#,
            )
            .unwrap()
            .file,
            FileTarget::Symbolic(PathBuf::from("~/.QuarticCat").into()),
        );
        assert_eq!(
            parse(
                r#"
                    [file]
                    target = '~/.QuarticCat'
                    type = 'template'
                "#,
            )
            .unwrap()
            .file,
            FileTarget::ComplexTemplate(PathBuf::from("~/.QuarticCat").into()),
        );
        assert_ne!(
            parse(
                r#"
                    [file]
                    target = '~/.QuarticCat'
                    type = 'template'
                    if = 'bash'
                "#,
            )
            .unwrap()
            .file,
            FileTarget::ComplexTemplate(PathBuf::from("~/.QuarticCat").into()),
        );
        assert_eq!(
            parse(
                r#"
                    [file]
                    target = '~/.QuarticCat'
                    type = 'symbolic'
                    append = 'whatever'
                "#,
            )
            .is_err(),
            true
        );
    }
}
