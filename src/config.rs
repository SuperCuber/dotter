use anyhow::{Context, Result};

use filesystem;
use toml::value::Table;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub type Files = BTreeMap<PathBuf, PathBuf>;
pub type Variables = Table;
pub type Helpers = BTreeMap<String, PathBuf>;

fn merge_configuration_tables(mut global: GlobalConfig, mut local: LocalConfig) -> GlobalConfig {
    // Apply packages filter
    global.packages = global
        .packages
        .into_iter()
        .filter(|(k, _)| local.packages.contains(&k))
        .collect();

    let mut output = GlobalConfig {
        helpers: global.helpers,
        packages: Default::default(),
    };

    for (package_name, mut package_global) in global.packages.into_iter() {
        // Extend it with the local patch
        if let Some(package_local) = local.package_patches.remove(&package_name) {
            package_global.files.extend(package_local.files);
            package_global.variables.extend(package_local.variables);

            // Remove files with target = ""
            package_global.files = package_global.files.into_iter().filter(|(_, v)| dbg!(v.to_string_lossy()) != "").collect();
        }

        // Insert into output
        output.packages.insert(package_name, package_global);
    }

    output
}

#[derive(Error, Debug)]
pub enum LoadConfigFailType {
    #[error("Failed to find config files")]
    Find,

    #[error("Failed to parse config file {file}")]
    Parse {
        file: PathBuf,
        source: filesystem::FileLoadError,
    },

    #[error("Failed to inspect source files")]
    InvalidSourceTree { source: anyhow::Error },
}

#[derive(Debug, Deserialize, Default)]
struct Package {
    #[serde(default)]
    files: Files,
    #[serde(default)]
    variables: Table,
}

#[derive(Debug, Deserialize)]
struct GlobalConfig {
    #[serde(default)]
    helpers: Helpers,
    #[serde(flatten)]
    packages: BTreeMap<String, Package>,
}

#[derive(Debug, Deserialize)]
struct LocalConfig {
    packages: Vec<String>,
    #[serde(flatten)]
    package_patches: BTreeMap<String, Package>,
}

fn try_load_configuration(
    local_config: &Path,
    global_config: &Path,
) -> Result<(Files, Variables, Helpers), LoadConfigFailType> {
    let global: GlobalConfig = match filesystem::load_file(global_config) {
        Err(filesystem::FileLoadError::Open { .. }) => Err(LoadConfigFailType::Find),
        Err(e) => Err(LoadConfigFailType::Parse {
            file: global_config.into(),
            source: e,
        }),
        Ok(global) => Ok(global),
    }?;

    debug!("Global config: {:?}", global);

    let local: LocalConfig =
        filesystem::load_file(local_config).map_err(|e| LoadConfigFailType::Parse {
            file: local_config.into(),
            source: e,
        })?;
    debug!("Local config: {:?}", local);

    let merged_config = merge_configuration_tables(global, local);
    debug!("Merged config: {:?}", merged_config);

    // Merge all the packages
    let Package { files, variables } = {
        let mut configuration_packages = merged_config.packages.into_iter();
        let mut first_package = configuration_packages
            .next()
            .unwrap_or_else(|| (String::new(), Package::default()))
            .1;
        for (_, v) in configuration_packages {
            first_package.files.extend(v.files);
            first_package.variables.extend(v.variables);
        }
        first_package
    };

    let files = expand_directories(files)
        .map_err(|e| LoadConfigFailType::InvalidSourceTree { source: e })?;
    debug!("Expanded files: {:?}", files);

    debug!("Final files: {:?}", files);
    debug!("Final variables: {:?}", variables);
    debug!("Final helpers: {:?}", merged_config.helpers);

    Ok((files, variables, merged_config.helpers))
}

fn expand_directories(files: Files) -> Result<Files> {
    let expanded = files
        .into_iter()
        .map(|(from, to)| {
            expand_directory(&from, &to).context(format!("Failed to expand file {:?}", from))
        })
        .collect::<Result<Vec<Files>>>()?;
    Ok(expanded.into_iter().flatten().collect::<Files>())
}

/// If a file is given, it will return a map of one element
/// Otherwise, returns recursively all the children and their targets
///  in relation to parent target
fn expand_directory(source: &Path, target: &Path) -> Result<Files> {
    if fs::metadata(source)
        .context(format!("Failed to read metadata of {:?}", source))?
        .is_file()
    {
        let mut map = Files::new();
        map.insert(source.into(), target.into());
        Ok(map)
    } else {
        let expanded = fs::read_dir(source)
            .context(format!("Failed to read contents of directory {:?}", source))?
            .map(|child| -> Result<Files> {
                let child = child?.file_name();
                let child_source = PathBuf::from(source).join(&child);
                let child_target = PathBuf::from(target).join(&child);
                expand_directory(&child_source, &child_target)
                    .context(format!("Failed to expand file {:?}", child_source))
            })
            .collect::<Result<Vec<Files>>>()?; // Use transposition of Iterator<Result<T,E>> -> Result<Sequence<T>, E>
        Ok(expanded.into_iter().flatten().collect())
    }
}

pub fn load_configuration(
    local_config: &Path,
    global_config: &Path,
) -> Result<(Files, Variables, Helpers), LoadConfigFailType> {
    let mut parent = ::std::env::current_dir().expect("Failed to get current directory.");
    let (files, variables, helpers) = loop {
        match try_load_configuration(local_config, global_config) {
            Ok(conf) => break Ok(conf),
            Err(LoadConfigFailType::Find) => {
                if let Some(new_parent) = parent.parent().map(|p| p.into()) {
                    parent = new_parent;
                    warn!(
                        "Didn't find configuration in current directory. Going one up to {:?}",
                        parent
                    );
                } else {
                    warn!("Reached root.");
                    break Err(LoadConfigFailType::Find);
                }
                ::std::env::set_current_dir(&parent).expect("Move a directory up");
            }
            Err(e) => break Err(e),
        }
    }?;

    let files = files
        .into_iter()
        .map(|(k, v)| {
            (k, shellexpand::tilde(&v.to_string_lossy()).to_string().into())
        })
        .collect();
    Ok((files, variables, helpers))
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Cache {
    pub symlinks: Files,
    pub templates: Files,
}

pub fn load_cache(cache: &Path) -> Result<Cache> {
    let cache: Cache = match filesystem::load_file(cache) {
        Ok(cache) => cache,
        Err(filesystem::FileLoadError::Open { .. }) => Cache::default(),
        Err(e) => Err(e).context("Failed to load cache file")?,
    };

    debug!("Cache {:?}", cache);

    Ok(cache)
}

pub fn save_cache(cache_file: &Path, cache: Cache) -> Result<()> {
    filesystem::save_file(cache_file, cache).context("Failed to save cache file")?;

    Ok(())
}
