use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::filesystem;

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum UnixUser {
    Uid(i32),
    Name(String),
}

impl fmt::Display for UnixUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnixUser::Uid(uid) => write!(f, "{uid}"),
            UnixUser::Name(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SymbolicTarget {
    pub target: PathBuf,
    pub owner: Option<UnixUser>,
    pub recurse: Option<bool>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(from = "FileTargetOuterRepr", into = "FileTargetOuterRepr")]
pub enum FileTarget {
    Automatic(PathBuf),
    Symbolic(SymbolicTarget),
    #[serde(rename = "template")]
    ComplexTemplate(TemplateTarget),
}

// Shims to allow Serde to represent FileTarget::Automatic as untagged while the
// remaining variants are differentiated by an internal tag as defined below
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum FileTargetOuterRepr {
    Simple(PathBuf),
    Complex(FileTargetInnerRepr),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FileTargetInnerRepr {
    Symbolic(SymbolicTarget),
    #[serde(rename = "template")]
    ComplexTemplate(TemplateTarget),
}

pub type Files = BTreeMap<PathBuf, FileTarget>;
pub type Variables = toml::value::Table;
#[cfg(feature = "scripting")]
pub type Helpers = BTreeMap<String, PathBuf>;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultTargetType {
    Symbolic,
    Template,
    #[default]
    Automatic,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Settings {
    #[serde(default)]
    default_target_type: DefaultTargetType,
}

#[derive(Debug, Clone)]
pub struct Configuration {
    pub files: Files,
    pub variables: Variables,
    pub packages: BTreeMap<String, bool>,

    #[cfg(feature = "scripting")]
    pub helpers: Helpers,

    /// If the source is a directory, or a symlink to a directory,
    /// and this option is true, the source will be recursed and
    /// turned into a list of all the files inside the structure that
    /// are readable.
    pub recurse: bool,

    #[allow(dead_code)]
    pub settings: Settings,
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
    #[cfg(feature = "scripting")]
    helpers: Helpers,
    #[serde(flatten)]
    packages: BTreeMap<String, Package>,
    #[serde(default)]
    settings: Settings,
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
        .with_context(|| format!("load global config {global_config:?}"))?;
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
        local_config_buf.set_file_name(format!("{hostname}.toml"));
    }

    let local: LocalConfig = filesystem::load_file(local_config_buf.as_path())
        .and_then(|c| c.ok_or_else(|| anyhow::anyhow!("file not found")))
        .with_context(|| format!("load local config {local_config:?}"))?;
    trace!("Local config: {:#?}", local);

    let mut merged_config =
        merge_configuration_files(global, local, patch).context("merge configuration files")?;
    trace!("Merged config: {:#?}", merged_config);

    debug!("Expanding files which are directories...");
    merged_config.files =
        expand_directories(&merged_config).context("expand files that are directories")?;

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
    #[cfg(feature = "scripting")]
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
        #[cfg(feature = "scripting")]
        helpers: Helpers::new(),
        packages,
        settings: Settings::default(),
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
    original: &mut toml::map::Map<String, toml::Value>,
    new: toml::map::Map<String, toml::Value>,
) {
    for (key, new_value) in new {
        if let Some(original_value) = original.get_mut(&key) {
            match (
                original_value.as_table().cloned(),
                new_value.as_table().cloned(),
            ) {
                (Some(mut original_table), Some(new_table)) => {
                    recursive_extend_map(&mut original_table, new_table);
                    *original_value = toml::Value::Table(original_table);
                }
                _ => *original_value = new_value.clone(),
            }
        } else {
            original.insert(key, new_value);
        }
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
            let mut included: IncludedConfig = filesystem::load_file(included_path)
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
                debug!("append unknown packages: {:?}", included.keys());
                global.packages.append(&mut included);
            }

            Ok(())
        }()
        .with_context(|| format!("including file {included_path:?}"))?;
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
                    .with_context(|| format!("get info of package {package}"))?
                    .depends
                    .clone(),
            );
        }
        package_count = enabled_packages.len();
        enabled_packages.extend(new_packages);
    }

    let packages_map = global
        .packages
        .keys()
        .map(|k| (k.to_string(), enabled_packages.contains(k)))
        .collect();

    // Apply packages filter
    global.packages.retain(|k, _| enabled_packages.contains(k));

    let mut output = Configuration {
        #[cfg(feature = "scripting")]
        helpers: global.helpers,
        files: Files::default(),
        variables: Variables::default(),
        packages: packages_map,
        recurse: true,
        settings: Settings::default(),
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
                }
                first_package.files.insert(file_name, file_target);
            }

            for (variable_name, variable_value) in package.variables {
                if let Some(first_value) = first_package.variables.get_mut(&variable_name) {
                    match (first_value, variable_value) {
                        (toml::Value::Table(first_value), toml::Value::Table(variable_value)) => {
                            trace!("Merging {:?} tables", variable_name);
                            recursive_extend_map(first_value, variable_value);
                        }
                        _ => {
                            anyhow::bail!("variable {:?} already encountered", variable_name);
                        }
                    }
                } else {
                    first_package
                        .variables
                        .insert(variable_name, variable_value);
                }
            }

            Ok(())
        }()
        .with_context(|| format!("merge package {package_name:?}"))?;
    }
    output.files = first_package.files;
    output.variables = first_package.variables;

    for value in output.files.values_mut() {
        if let FileTarget::Automatic(target) = value {
            *value = match global.settings.default_target_type {
                DefaultTargetType::Symbolic => {
                    FileTarget::Symbolic(SymbolicTarget::from(target.clone()))
                }
                DefaultTargetType::Template => {
                    FileTarget::ComplexTemplate(TemplateTarget::from(target.clone()))
                }
                _ => continue,
            };
        }
    }

    // Add local.toml's patches
    output.files.extend(local.files);
    recursive_extend_map(&mut output.variables, local.variables);

    // Add manual patch
    if let Some(patch) = patch {
        output.files.extend(patch.files);
        recursive_extend_map(&mut output.variables, patch.variables);
    }

    // Remove files with target = ""
    output.files.retain(|_, v| v.path().to_string_lossy() != "");

    Ok(output)
}

impl FileTarget {
    pub fn path(&self) -> &Path {
        match self {
            FileTarget::Automatic(path) => path,
            FileTarget::Symbolic(SymbolicTarget { target, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { target, .. }) => target,
        }
    }

    pub fn set_path(&mut self, new_path: impl Into<PathBuf>) {
        match self {
            FileTarget::Automatic(path) => *path = new_path.into(),
            FileTarget::Symbolic(SymbolicTarget { target, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { target, .. }) => {
                *target = new_path.into();
            }
        }
    }

    pub fn condition(&self) -> Option<&String> {
        match self {
            FileTarget::Automatic(_) => None,
            FileTarget::Symbolic(SymbolicTarget { condition, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { condition, .. }) => condition.as_ref(),
        }
    }
}

impl<T: Into<PathBuf>> From<T> for FileTarget {
    fn from(input: T) -> Self {
        FileTarget::Automatic(input.into())
    }
}

impl From<FileTargetOuterRepr> for FileTarget {
    fn from(input: FileTargetOuterRepr) -> Self {
        use FileTargetInnerRepr as IR;
        use FileTargetOuterRepr as OR;
        match input {
            OR::Simple(x) => Self::Automatic(x),
            OR::Complex(IR::Symbolic(x)) => Self::Symbolic(x),
            OR::Complex(IR::ComplexTemplate(x)) => Self::ComplexTemplate(x),
        }
    }
}

impl From<FileTarget> for FileTargetOuterRepr {
    fn from(input: FileTarget) -> Self {
        use FileTargetInnerRepr as IR;
        match input {
            FileTarget::Automatic(x) => Self::Simple(x),
            FileTarget::Symbolic(x) => Self::Complex(IR::Symbolic(x)),
            FileTarget::ComplexTemplate(x) => Self::Complex(IR::ComplexTemplate(x)),
        }
    }
}

impl<T: Into<PathBuf>> From<T> for SymbolicTarget {
    fn from(input: T) -> Self {
        SymbolicTarget {
            target: input.into(),
            owner: None,
            condition: None,
            recurse: None,
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

fn expand_directories(config: &Configuration) -> Result<Files> {
    let expanded = config
        .files
        .iter()
        .map(|(source, target)| {
            expand_directory(source, target, config).context(format!("expand file {source:?}"))
        })
        .collect::<Result<Vec<Files>>>()?;
    Ok(expanded.into_iter().flatten().collect::<Files>())
}

/// If a file is given, it will return a map of one element
/// Otherwise, returns recursively all the children and their targets
/// in relation to parent target
fn expand_directory(source: &Path, target: &FileTarget, config: &Configuration) -> Result<Files> {
    let metadata = fs::metadata(source).context("read file metadata")?;

    // if a target explicitly specifies a recurse option, this takes
    // precedence over the global default
    let recurse = match target {
        FileTarget::Symbolic(SymbolicTarget {
            target: _,
            owner: _,
            condition: _,
            recurse: Some(rec),
        }) => *rec,
        _ => config.recurse,
    };

    trace!("expanding '{source:?}', recurse: {recurse}");

    if !recurse || !metadata.is_dir() {
        let mut map = Files::new();
        map.insert(source.into(), target.clone());
        Ok(map)
    } else {
        let expanded = fs::read_dir(source)
            .context("read contents of directory")?
            .map(|child| -> Result<Files> {
                let child = child?.file_name();
                let child_source = PathBuf::from(source).join(&child);
                let mut child_target = target.clone();
                child_target.set_path(child_target.path().join(&child));
                expand_directory(&child_source, &child_target, config)
                    .context(format!("expand file {child_source:?}"))
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
            UnixUser::Uid(id) => format!("#{id}"),
        }
    }

    pub fn as_chown_arg(&self) -> String {
        match self {
            UnixUser::Name(n) => n.clone(),
            UnixUser::Uid(id) => format!("{id}"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize_file_target() {
        #[derive(Debug, Deserialize)]
        struct Helper {
            file: FileTarget,
        }

        let parse = toml::from_str::<Helper>;

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
        parse(
            r#"
                [file]
                target = '~/.QuarticCat'
                type = 'symbolic'
                append = 'whatever'
            "#,
        )
        .unwrap_err();
    }

    #[test]
    fn settting_default_target_type_symbolic() {
        let global: GlobalConfig = toml::from_str(
            r#"
                [settings]
                default_target_type = "symbolic"

                [cat]
                depends = []

                [cat.files]
                cat = '~/.QuarticCat'

                [derby]
                depends = []

                [derby.files]
                derby = { target = '~/.DerbyLantern', type = 'template' }
            "#,
        )
        .unwrap();

        let local: LocalConfig = toml::from_str(
            r#"
               packages = ['cat', 'derby']
           "#,
        )
        .unwrap();

        let merged_config = merge_configuration_files(global, local, None);

        let config = merged_config.unwrap();

        let cat = config.files.get(&PathBuf::from("cat")).unwrap();

        let derby = config.files.get(&PathBuf::from("derby")).unwrap();

        assert_eq!(
            cat,
            &FileTarget::Symbolic(PathBuf::from("~/.QuarticCat").into())
        );

        assert_ne!(
            derby,
            &FileTarget::Symbolic(PathBuf::from("~/.DerbyLantern").into())
        );

        assert_eq!(
            derby,
            &FileTarget::ComplexTemplate(PathBuf::from("~/.DerbyLantern").into())
        );
    }

    #[test]
    fn setting_default_target_type_template() {
        let global: GlobalConfig = toml::from_str(
            r#"
                [settings]
                default_target_type = "template"

                [cat]
                depends = []

                [cat.files]
                cat = '~/.QuarticCat'

                [derby]
                depends = []

                [derby.files]
                derby = { target = '~/.DerbyLantern', type = 'symbolic' }
            "#,
        )
        .unwrap();

        let local: LocalConfig = toml::from_str(
            r#"
               packages = ['cat', 'derby']
           "#,
        )
        .unwrap();

        let merged_config = merge_configuration_files(global, local, None);

        let config = merged_config.unwrap();

        let cat = config.files.get(&PathBuf::from("cat")).unwrap();

        let derby = config.files.get(&PathBuf::from("derby")).unwrap();

        assert_eq!(
            cat,
            &FileTarget::ComplexTemplate(PathBuf::from("~/.QuarticCat").into())
        );

        assert_ne!(
            derby,
            &FileTarget::ComplexTemplate(PathBuf::from("~/.DerbyLantern").into())
        );

        assert_eq!(
            derby,
            &FileTarget::Symbolic(PathBuf::from("~/.DerbyLantern").into())
        );
    }

    #[test]
    fn setting_default_target_type_automatic() {
        let global: GlobalConfig = toml::from_str(
            r#"
                [cat]
                depends = []

                [cat.files]
                cat = '~/.QuarticCat'

                [derby]
                depends = []

                [derby.files]
                derby = { target = '~/.DerbyLantern', type = 'template' }

                [sliver]
                depends = []

                [sliver.files]
                sliver = { target = '~/.SliverBodacious', type = 'symbolic' }
            "#,
        )
        .unwrap();

        let local: LocalConfig = toml::from_str(
            r#"
               packages = ['cat', 'derby', 'sliver']
           "#,
        )
        .unwrap();

        let merged_config = merge_configuration_files(global, local, None);

        let config = merged_config.unwrap();

        let cat = config.files.get(&PathBuf::from("cat")).unwrap();

        let derby = config.files.get(&PathBuf::from("derby")).unwrap();

        let sliver = config.files.get(&PathBuf::from("sliver")).unwrap();

        assert_eq!(cat, &FileTarget::Automatic(PathBuf::from("~/.QuarticCat")));

        assert_eq!(
            derby,
            &FileTarget::ComplexTemplate(PathBuf::from("~/.DerbyLantern").into())
        );

        assert_eq!(
            sliver,
            &FileTarget::Symbolic(PathBuf::from("~/.SliverBodacious").into())
        );
    }
}
