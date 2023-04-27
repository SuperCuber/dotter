use std::collections::{BTreeMap, HashSet};

use anyhow::Context;
use anyhow::Result;
use dialoguer::MultiSelect;

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

    let mut multi_select = MultiSelect::new();

    return if opt.local_config.exists() {
        debug!(
            "Local configuration file found at {}",
            opt.local_config.display()
        );

        let mut local_config: LocalConfig = load_local_config(&opt.local_config)?;

        let enabled_packages = &local_config.packages;

        // all packages, including the ones that are disabled
        for (package_name, dependencies) in packages.iter() {
            multi_select.item_checked(
                format_package(package_name, dependencies),
                enabled_packages.contains(package_name),
            );
        }

        println!("Use space to select packages to enable, and enter to confirm");
        let selected_items = multi_select.interact_opt()?;
        trace!("Selected elements: {:?}", selected_items);

        match selected_items {
            Some(selected_items) => {
                modify_and_save(
                    opt,
                    &mut local_config,
                    packages.iter().map(|(key, _)| key).collect(),
                    selected_items,
                )?;
            }
            None => {
                // user pressed "Esc" or "q" to quit
                println!("Aborting.");
            }
        }
        Ok(false)
    } else {
        debug!(
            "No local configuration file found at {}",
            opt.local_config.display()
        );
        trace!("Available packages: {:?}", packages);

        println!("Use space to select packages to enable, and enter to confirm");
        let selected_elements = multi_select
            .with_prompt("Select packages to install")
            .items(
                packages
                    .iter()
                    .map(|(key, value)| format_package(key, value))
                    .collect::<Vec<String>>()
                    .iter()
                    .as_slice(),
            )
            .interact_opt()?;
        trace!("Selected elements: {:?}", selected_elements);

        match selected_elements {
            Some(selected_elements) if selected_elements.is_empty() => {
                println!("No packages selected, writing empty configuration");
                filesystem::save_file(opt.local_config.as_path(), LocalConfig::empty_config())
                    .context("Writing empty configuration")?;
            }
            Some(selected_elements) => {
                modify_and_save(
                    opt,
                    &mut LocalConfig::empty_config(),
                    packages
                        .iter()
                        .map(|(key, _)| key)
                        .collect::<Vec<&String>>(),
                    selected_elements,
                )?;
            }
            None => {
                // user pressed "Esc" or "q" to quit
                println!("Aborting.");
            }
        }
        Ok(false)
    };
}

fn format_package(package_name: &String, dependencies: &Vec<String>) -> String {
    let dependencies_string = if !dependencies.is_empty() {
        format!(" # (will enable {})", dependencies.join(", "))
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
    opt: &Options,
    local_config: &mut LocalConfig,
    items_in_order: Vec<&String>,
    selected_items: Vec<usize>,
) -> Result<()> {
    println!("Writing configuration to {}", opt.local_config.display());
    trace!(
        "Selected indexes: {:?} of {:?}",
        selected_items,
        items_in_order
    );

    local_config.packages = selected_items
        .iter()
        .map(|i| items_in_order[*i].clone())
        .collect::<Vec<String>>();

    filesystem::save_file(&opt.local_config, local_config).context("Writing local config to file")
}
