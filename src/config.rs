use filesystem;
use toml::value::Table;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io, process};

pub type Files = BTreeMap<String, String>;
pub type FilesPath = BTreeMap<PathBuf, PathBuf>;
pub type Variables = Table;
pub type Helpers = BTreeMap<String, String>;

fn normalize_package_table(mut package: Table) -> Table {
    package
        .entry("files".into())
        .or_insert_with(|| toml::Value::Table(Table::new()));
    package
        .entry("variables".into())
        .or_insert_with(|| toml::Value::Table(Table::new()));
    package
}

fn merge_configuration_tables(global: Table, local: Table) -> Result<Table, String> {
    let mut output = Table::new();

    for (package_name, package_global) in global.into_iter() {
        // Normalize package_global
        let mut package_global = normalize_package_table(
            package_global
                .as_table()
                .ok_or(format!("Package {} is not a table", package_name))?
                .clone(),
        );

        // Extend it with normalized package_local
        if let Some(package_local) = local.get(&package_name) {
            let mut package_local = normalize_package_table(
                package_local
                    .as_table()
                    .ok_or(format!("Package {} is not a table", package_name))?
                    .clone(),
            );

            package_global
                .get_mut("files")
                .unwrap()
                .as_table_mut()
                .unwrap()
                .extend(
                    package_local
                        .get_mut("files")
                        .unwrap()
                        .as_table()
                        .unwrap()
                        .clone(),
                );

            package_global
                .get_mut("variables")
                .unwrap()
                .as_table_mut()
                .unwrap()
                .extend(
                    package_local
                        .get_mut("variables")
                        .unwrap()
                        .as_table()
                        .unwrap()
                        .clone(),
                );
        }

        // Insert into output
        output.insert(package_name, toml::Value::Table(package_global));
    }

    Ok(output)
}

// Returns a tuple of files and variables
fn parse_configuration_table(table: Table) -> BTreeMap<String, (Files, Variables)> {
    table
        .iter()
        .map(|(package_key, package_value)| {
            let mut files = Files::new();
            if let Some(files_table) = package_value.get("files").and_then(|f| f.as_table()) {
                for (from, to) in files_table.iter() {
                    if to.is_str() {
                        files.insert(from.to_string(), to.as_str().unwrap().to_string());
                    } else if to.is_bool() && !to.as_bool().unwrap() {
                        continue;
                    } else {
                        warn!(
                            "In package {} file {}, value {} is invalid.",
                            package_key, from, to
                        );
                    }
                }
            }

            let variables = package_value
                .get("variables")
                .and_then(|f| f.as_table())
                .map(|f| f.to_owned())
                .unwrap_or_else(Variables::new);

            (package_key.to_string(), (files, variables))
        })
        .collect()
}

/// Returns a tuple of (files, variables) if successful
fn try_load_configuration(
    local_config: &Path,
    global_config: &Path,
) -> Result<(FilesPath, Variables, Helpers), String> {
    let mut global: Table =
        filesystem::load_file(global_config).map_err(|e| format!("global: {}", e))?;

    // Get helpers, remove it from global so it isn't processed as a package
    let helpers = global
        .remove("helpers".into())
        .unwrap_or(Helpers::new().into())
        .as_table()
        .unwrap_or_else(|| {
            error!("'helpers' in global configuration is not a table");
            process::exit(1);
        })
        .into_iter()
        .map(|(helper_name, helper_location)| {
            Some((
                helper_name.to_string(),
                helper_location.as_str()?.to_string(),
            ))
        })
        .collect::<Option<Helpers>>()
        .unwrap_or_else(|| {
            error!("Some helper locations are not a string");
            process::exit(1);
        });
    debug!("Global: {:?}", global);
    debug!("Helpers: {:?}", helpers);

    let local: Table = filesystem::load_file(local_config).map_err(|e| format!("local: {}", e))?;
    debug!("Local: {:?}", local);

    let packages = local
        .get("packages")
        .and_then(|v| v.as_array())
        .and_then(|v| v.iter().map(|i| i.as_str()).collect::<Option<Vec<&str>>>())
        .unwrap_or_else(|| {
            error!("Failed to get array of packages (strings) from local configuration");
            process::exit(1);
        });
    debug!("Packages: {:?}", packages);

    // Apply packages filter
    let global: Table = global
        .into_iter()
        .filter(|(k, _)| packages.contains(&k.as_str()))
        .collect();
    debug!("Global after packages filtered: {:?}", global);

    // Normalize, parse, and merge the configuration files
    let configuration_packages =
        parse_configuration_table(merge_configuration_tables(global, local)?);

    // Merge all the packages
    let (files, variables) = {
        let mut configuration_packages = configuration_packages.into_iter();
        let mut first_package = configuration_packages
            .next()
            .expect("at least one package")
            .1;
        for (_, v) in configuration_packages {
            first_package.0.extend(v.0);
            first_package.1.extend(v.1);
        }
        first_package
    };

    let files = expand_directories(files).unwrap_or_else(|e| {
        error!(
            "Failed to expand directory sources into their children: {}",
            e
        );
        process::exit(1);
    });
    debug!("Expanded files: {:?}", files);

    debug!("Final configuration: {:?}", (&files, &variables, &helpers));

    Ok((files, variables, helpers))
}

fn expand_directories(files: Files) -> io::Result<FilesPath> {
    let expanded = files
        .into_iter()
        .map(|(from, to)| expand_directory(&PathBuf::from(from), &PathBuf::from(to)))
        .collect::<io::Result<Vec<FilesPath>>>()?;
    Ok(expanded.into_iter().flatten().collect::<FilesPath>())
}

/// If a file is given, it will return a map of one element
/// Otherwise, returns recursively all the children and their targets
///  in relation to parent target
fn expand_directory(source: &Path, target: &Path) -> io::Result<FilesPath> {
    // TODO: might wanna swap all this to expects and unwraps or some other in-place crash
    // because otherwise error reporting is really undescriptive
    if fs::metadata(source)?.is_file() {
        let mut map = FilesPath::new();
        map.insert(source.into(), target.into());
        Ok(map)
    } else {
        let expanded = fs::read_dir(source)?
            .map(|child| -> io::Result<FilesPath> {
                let child = child?.file_name();
                let child_source = PathBuf::from(source).join(&child);
                let child_target = PathBuf::from(target).join(&child);
                expand_directory(&child_source, &child_target)
            })
            .collect::<io::Result<Vec<FilesPath>>>()?;
        Ok(expanded.into_iter().flatten().collect())
    }
}

pub fn load_configuration(
    local_config: &Path,
    global_config: &Path,
) -> Option<(FilesPath, Variables, Helpers)> {
    let mut parent = ::std::env::current_dir().expect("Failed to get current directory.");
    loop {
        match try_load_configuration(local_config, global_config) {
            Ok(conf) => break Some(conf),
            Err(e) => {
                if let Some(new_parent) = parent.parent().map(|p| p.into()) {
                    parent = new_parent;
                    warn!(
                        "Current directory failed on step: {}, going one up to {:?}",
                        e, parent
                    );
                } else {
                    warn!("Reached root.");
                    break None;
                }
                ::std::env::set_current_dir(&parent).expect("Move a directory up");
            }
        }
    }
}

/// Returns tuple of (existing_symlinks, existing_templates)
pub fn load_cache(cache: &Path) -> (FilesPath, FilesPath) {
    let file: Table = filesystem::load_file(cache).unwrap_or_else(|e| {
        error!("Failed to load cache file {:?}: {}", cache, e);
        process::exit(1);
    });

    let symlinks = file
        .get("symlinks")
        .and_then(|v| v.as_table())
        .expect("symlinks table in cache file")
        .to_owned()
        .into_iter()
        .map(|(k, v)| {
            (
                PathBuf::from(k),
                PathBuf::from(v.as_str().expect("value is string")),
            )
        })
        .collect::<FilesPath>();

    let templates = file
        .get("templates")
        .and_then(|v| v.as_table())
        .expect("symlinks table in cache file")
        .to_owned()
        .into_iter()
        .map(|(k, v)| {
            (
                PathBuf::from(k),
                PathBuf::from(v.as_str().expect("value is string")),
            )
        })
        .collect::<FilesPath>();

    (symlinks, templates)
}
