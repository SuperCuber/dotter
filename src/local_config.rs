use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::Result;
use anyhow::{Context, Error};
use crossterm::style::{style, Color, Stylize};
use dialoguer::{MultiSelectPlus, MultiSelectPlusItem, MultiSelectPlusStatus};

use crate::args::Options;
use crate::config::{load_global_config, load_local_config, GlobalConfig, LocalConfig, Package};
use crate::filesystem;

/// Returns true if an error was printed
pub fn config(opt: &Options) -> Result<bool> {
    let global_config: GlobalConfig = load_global_config(&opt.global_config)?;

    let mut visited = HashSet::new();
    let mut packages = vec![];
    for package in global_config.packages.values() {
        let dependency_names =
            get_package_dependencies(&global_config.packages, package, &mut visited, 0, 6);
        debug!("Generated dependency names: {:#?}", dependency_names);
        packages.extend(dependency_names);
        visited.clear();
    }

    trace!("Available packages: {:?}", packages);

    let multi_select = MultiSelectPlus::new();

    let enabled_packages = if opt.local_config.exists() {
        debug!(
            "Local configuration file found at {}",
            opt.local_config.display()
        );

        let local_config: LocalConfig = load_local_config(&opt.local_config)?;
        trace!("Local configuration: {:?}", local_config);
        local_config.packages
    } else {
        debug!(
            "No local configuration file found at {}",
            opt.local_config.display()
        );
        // no local config => no packages are enabled
        Vec::new()
    };
    let selected_items = prompt(multi_select, &packages, &enabled_packages)?;
    trace!("Selected elements: {:?}", selected_items);
    write_selected_elements(&opt.local_config, selected_items, &packages, false)?;

    Ok(false)
}

fn prompt(
    multi_select: MultiSelectPlus,
    packages: &[PackageNames],
    enabled_packages: &[String],
) -> dialoguer::Result<Option<Vec<usize>>> {
    return multi_select
        .with_prompt("Select packages to install")
        .items(
            packages
                .iter()
                .map(|(key, value)| MultiSelectPlusItem {
                    name: format_package(key, value),
                    status: if enabled_packages.contains(key) {
                        MultiSelectPlusStatus::CHECKED
                    } else if is_transitive_dependency(key, packages, enabled_packages) {
                        MultiSelectPlusStatus {
                            checked: false,
                            symbol: "-",
                        }
                    } else {
                        MultiSelectPlusStatus::UNCHECKED
                    },
                    summary_text: key.clone(),
                })
                .collect::<Vec<_>>(),
        )
        .interact_opt();
}

// checks if a package is a transitive dependency of an enabled package
fn is_transitive_dependency(
    package_name: &String,
    packages: &[PackageNames],
    enabled_packages: &[String],
) -> bool {
    packages
        .iter()
        .filter(|(key, _)| enabled_packages.contains(key))
        .any(|(_, dependencies)| dependencies.contains(package_name))
}

fn write_selected_elements(
    config_path: &Path,
    selected_elements: Option<Vec<usize>>,
    packages: &[PackageNames],
    write_empty: bool,
) -> Result<(), Error> {
    return match selected_elements {
        Some(selected_elements) if selected_elements.is_empty() && write_empty => {
            println!("No packages selected, writing empty configuration");
            filesystem::save_file(config_path, LocalConfig::empty_config())
                .context("Writing empty configuration")
        }
        Some(selected_elements) => modify_and_save(
            config_path,
            &mut LocalConfig::empty_config(),
            packages
                .iter()
                .map(|(key, _)| key)
                .collect::<Vec<&String>>(),
            selected_elements,
        ),
        None => {
            // user pressed "Esc" or "q" to quit
            println!("Aborting.");
            Ok(())
        }
    };
}

fn format_package(package_name: &String, dependencies: &Vec<String>) -> String {
    let dependencies_string = if !dependencies.is_empty() {
        style(format!(" # (will enable {})", dependencies.join(", ")))
            // fallback for terms not supporting 8-bit ANSI
            .with(Color::White)
            .with(Color::AnsiValue(244))
            .to_string()
    } else {
        String::new()
    };
    format!("{package_name}{dependencies_string}")
}

/// (package_name, dependencies)
type PackageNames = (String, Vec<String>);

fn get_package_dependencies<'a>(
    package_map: &'a BTreeMap<String, Package>,
    package: &'a Package,
    visited: &mut HashSet<&'a Package>,
    depth: usize,
    max_depth: usize,
) -> Vec<PackageNames> {
    if depth > max_depth {
        return Vec::new();
    }

    let mut result = Vec::new();
    if visited.contains(&package) {
        // Avoid infinite recursion caused by circular dependencies
        return result;
    }
    visited.insert(package);

    let package_name = package_map.iter().find(|(_, p)| *p == package).unwrap().0;

    let mut dependencies = Vec::new();
    for dep_name in &package.depends {
        if let Some(dep_package) = package_map.get(dep_name) {
            let dep_desc =
                get_package_dependencies(package_map, dep_package, visited, depth + 1, max_depth);
            for (dep_name, dep_deps) in dep_desc {
                dependencies.push(dep_name);
                dependencies.extend(dep_deps);
            }
        }
    }

    let package_names = (package_name.to_owned(), dependencies);
    result.push(package_names);

    visited.remove(&package);
    result
}

fn modify_and_save(
    config_path: &Path,
    local_config: &mut LocalConfig,
    items_in_order: Vec<&String>,
    selected_items: Vec<usize>,
) -> Result<()> {
    println!("Writing configuration to {}", config_path.display());
    trace!(
        "Selected indexes: {:?} of {:?}",
        selected_items,
        items_in_order
    );

    local_config.packages = selected_items
        .iter()
        .map(|i| items_in_order[*i].clone())
        .collect::<Vec<String>>();

    filesystem::save_file(config_path, local_config).context("Writing local config to file")
}
