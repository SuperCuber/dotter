use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};

use crate::filesystem;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum UnixUser {
    Uid(i32),
    Name(String),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SymbolicTarget {
    pub target: PathBuf,
    pub owner: Option<UnixUser>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TemplateTarget {
    pub target: PathBuf,
    pub owner: Option<UnixUser>,
    pub append: Option<String>,
    pub prepend: Option<String>,
}

// Deserialize implemented manually
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
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
        .with_context(|| format!("load global config {:?}", global_config))?;
    trace!("Global config: {:#?}", global);

    let local: LocalConfig = filesystem::load_file(local_config)
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
        .map(|(k, v)| {
            (
                k,
                v.map(|path| {
                    shellexpand::tilde(&path.to_string_lossy())
                        .to_string()
                        .into()
                }),
            )
        })
        .collect();

    trace!("Final files: {:#?}", merged_config.files);
    trace!("Final variables: {:#?}", merged_config.variables);
    trace!("Final helpers: {:?}", merged_config.helpers);

    Ok(merged_config)
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    pub symlinks: BTreeMap<PathBuf, PathBuf>,
    pub templates: BTreeMap<PathBuf, PathBuf>,
}

pub fn load_cache(cache: &Path) -> Result<Option<Cache>> {
    debug!("Loading cache...");

    let cache = match filesystem::load_file(cache) {
        Ok(cache) => Some(cache),
        Err(filesystem::FileLoadError::Open { .. }) => None,
        Err(e) => Err(e).context("load cache file")?,
    };

    trace!("Cache: {:#?}", cache);

    Ok(cache)
}

pub fn save_cache(cache_file: &Path, cache: Cache) -> Result<()> {
    debug!("Saving cache...");
    filesystem::save_file(cache_file, cache).context("save cache")?;

    Ok(())
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
            let mut included: IncludedConfig =
                filesystem::load_file(&included_path).context("load file")?;

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

    // Apply packages filter
    global.packages = global
        .packages
        .into_iter()
        .filter(|(k, _)| local.packages.contains(&k))
        .collect();

    let mut output = Configuration {
        helpers: global.helpers,
        files: Files::default(),
        variables: Variables::default(),
        packages: local.packages,
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

impl<'de> serde::Deserialize<'de> for FileTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Target,
            Owner,
            Append,
            Prepend,
            Type,
        }

        struct FileTargetVisitor;

        impl<'de> serde::de::Visitor<'de> for FileTargetVisitor {
            type Value = FileTarget;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a string or a map")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(FileTarget::Automatic(s.into()))
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut file_type = None;
                let mut target = None;
                let mut owner = None;
                let mut append = None;
                let mut prepend = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Type => {
                            if file_type.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            file_type = Some(map.next_value()?);
                        }
                        Field::Target => {
                            if target.is_some() {
                                return Err(serde::de::Error::duplicate_field("target"));
                            }
                            target = Some(map.next_value()?);
                        }
                        Field::Owner => {
                            if owner.is_some() {
                                return Err(serde::de::Error::duplicate_field("owner"));
                            }
                            owner = Some(map.next_value()?);
                        }
                        Field::Append => {
                            if append.is_some() {
                                return Err(serde::de::Error::duplicate_field("append"));
                            }
                            append = Some(map.next_value()?);
                        }
                        Field::Prepend => {
                            if prepend.is_some() {
                                return Err(serde::de::Error::duplicate_field("prepend"));
                            }
                            prepend = Some(map.next_value()?);
                        }
                    }
                }

                let file_type = file_type.ok_or_else(|| serde::de::Error::missing_field("type"))?;
                let target = target.ok_or_else(|| serde::de::Error::missing_field("target"))?;
                let ans = match file_type {
                    "symbolic" => {
                        if append.is_some() || prepend.is_some() {
                            return Err(serde::de::Error::custom(
                                "invalid use of `append` or `prepend` on a symbolic target",
                            ));
                        }
                        FileTarget::Symbolic(SymbolicTarget { target, owner })
                    }
                    "template" => FileTarget::ComplexTemplate(TemplateTarget {
                        target,
                        owner,
                        append,
                        prepend,
                    }),
                    other_type => {
                        return Err(serde::de::Error::invalid_value(
                            serde::de::Unexpected::Str(other_type),
                            &"`symbolic` or `template`",
                        ))
                    }
                };

                Ok(ans)
            }
        }

        deserializer.deserialize_any(FileTargetVisitor)
    }
}

impl FileTarget {
    fn map<F: FnOnce(PathBuf) -> PathBuf>(self, func: F) -> Self {
        match self {
            FileTarget::Automatic(path) => FileTarget::Automatic(func(path)),
            FileTarget::Symbolic(mut s) => {
                s.target = func(s.target);
                FileTarget::Symbolic(s)
            }
            FileTarget::ComplexTemplate(mut t) => {
                t.target = func(t.target);
                FileTarget::ComplexTemplate(t)
            }
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            FileTarget::Automatic(path) => &path,
            FileTarget::Symbolic(SymbolicTarget { target, .. })
            | FileTarget::ComplexTemplate(TemplateTarget { target, .. }) => &target,
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
        }
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
        let target = match target {
            FileTarget::Automatic(target) => target,
            _ => anyhow::bail!("Complex file target not implemented for directories yet."),
        };
        let expanded = fs::read_dir(source)
            .context("read contents of directory")?
            .map(|child| -> Result<Files> {
                let child = child?.file_name();
                let child_source = PathBuf::from(source).join(&child);
                let child_target = target.clone().join(&child);
                expand_directory(&child_source, child_target.into())
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
