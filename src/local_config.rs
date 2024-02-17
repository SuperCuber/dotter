use std::collections::{BTreeMap, BTreeSet};
use std::iter::FromIterator;
use std::path::Path;

use anyhow::Result;
use anyhow::{Context, Error};
use crossterm::style::{style, Color, Stylize};
use dialoguer::{MultiSelectPlus, MultiSelectPlusItem, MultiSelectPlusStatus, SelectCallback};

use crate::args::Options;
use crate::config::{load_global_config, load_local_config, GlobalConfig, LocalConfig, Package};
use crate::filesystem;

const DEPENDENCY: MultiSelectPlusStatus = MultiSelectPlusStatus {
    checked: false,
    symbol: "-",
};

/// Returns true if an error was printed
pub fn config(opt: &Options) -> Result<bool> {
    let global_config: GlobalConfig = load_global_config(&opt.global_config)?;

    let packages = global_config
        .packages
        .iter()
        .map(|(name, package)| {
            let mut visited = BTreeSet::new();
            let dependencies =
                get_package_dependencies(&global_config.packages, name, package, &mut visited);
            (name.clone(), dependencies)
        })
        .collect::<BTreeMap<String, Vec<String>>>();

    trace!("Available packages: {:?}", packages);

    // TODO: this check will fail if the local config is from <hostname>.toml
    // The solution is to add better error type to load_local_config and match on it here
    let enabled_packages = if opt.local_config.exists() {
        debug!(
            "Local configuration file found at {}",
            opt.local_config.display()
        );

        let local_config: LocalConfig = load_local_config(&opt.local_config)?;
        trace!("Local configuration: {:?}", local_config);
        BTreeSet::from_iter(local_config.packages)
    } else {
        debug!(
            "No local configuration file found at {}",
            opt.local_config.display()
        );
        // no local config => no packages are enabled
        BTreeSet::new()
    };

    let multi_select = MultiSelectPlus::new().with_select_callback(select_callback(&packages));

    let selected_items = prompt(multi_select, &packages, &enabled_packages)?;
    trace!("Selected elements: {:?}", selected_items);
    // TODO: "write_empty" is always false, do we want a flag for it? Or should we always write
    write_selected_elements(&opt.local_config, selected_items, &packages, false)?;

    Ok(false)
}

fn select_callback(packages: &BTreeMap<String, Vec<String>>) -> Box<SelectCallback> {
    Box::new(move |_, items| {
        // update the status of the items, making ones that were enabled through a transitive
        // dependency set to unchecked
        let enabled_packages = items
            .iter()
            .filter_map(|item| {
                if item.status.checked {
                    Some(item.summary_text.clone())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        let new_items = items
            .iter()
            .map(|item| {
                let Some(package) = packages.get_key_value(&item.summary_text) else {
                    // items that are not in the package list are just cloned as that
                    return item.clone();
                };

                if is_transitive_dependency(&item.summary_text, packages, &enabled_packages)
                    && item.status != MultiSelectPlusStatus::CHECKED
                {
                    // items that are enabled due to a transitive dependency
                    // CHECKED is excluded because it means the user explicitly enabled it
                    MultiSelectPlusItem {
                        name: format_package(package.0, package.1),
                        status: DEPENDENCY,
                        summary_text: item.summary_text.clone(),
                    }
                } else if item.status.symbol == "-" {
                    // previous transitive dependencies that are now unchecked
                    MultiSelectPlusItem {
                        name: format_package(package.0, package.1),
                        status: MultiSelectPlusStatus::UNCHECKED,
                        summary_text: item.summary_text.clone(),
                    }
                } else {
                    // checked or unchecked items are just cloned as that
                    item.clone()
                }
            })
            .collect();
        Some(new_items)
    })
}

fn prompt(
    multi_select: MultiSelectPlus,
    packages: &BTreeMap<String, Vec<String>>,
    enabled_packages: &BTreeSet<String>,
) -> dialoguer::Result<Option<Vec<usize>>> {
    multi_select
        .with_prompt("Select packages to install")
        .items(
            packages
                .iter()
                .map(|(key, value)| MultiSelectPlusItem {
                    name: format_package(key, value),
                    status: if enabled_packages.contains(key) {
                        MultiSelectPlusStatus::CHECKED
                    } else if is_transitive_dependency(key, packages, enabled_packages) {
                        DEPENDENCY
                    } else {
                        MultiSelectPlusStatus::UNCHECKED
                    },
                    summary_text: key.clone(),
                })
                .collect::<Vec<_>>(),
        )
        .interact_opt()
}

/// checks if a package is a transitive dependency of an enabled package
fn is_transitive_dependency(
    package_name: &String,
    packages: &BTreeMap<String, Vec<String>>,
    enabled_packages: &BTreeSet<String>,
) -> bool {
    packages
        .iter()
        .filter(|(key, _)| enabled_packages.contains(*key))
        .any(|(_, dependencies)| dependencies.contains(package_name))
}

fn write_selected_elements(
    config_path: &Path,
    selected_elements: Option<Vec<usize>>,
    packages: &BTreeMap<String, Vec<String>>,
    write_empty: bool,
) -> Result<(), Error> {
    match selected_elements {
        // TODO: In both of these cases, we should load the current local config and modify it,
        // since there are other options there. Maybe there shouldn't be a separate `if empty` case
        Some(selected_elements) if selected_elements.is_empty() && write_empty => {
            println!("No packages selected, writing empty configuration");
            filesystem::save_file(config_path, LocalConfig::default())
                .context("Writing empty configuration")
        }
        Some(selected_elements) => modify_and_save(
            config_path,
            &mut LocalConfig::default(),
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
    }
}

fn format_package(package_name: &String, dependencies: &[String]) -> String {
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

fn get_package_dependencies(
    package_map: &BTreeMap<String, Package>,
    package_name: &str,
    package: &Package,
    visited: &mut BTreeSet<String>,
) -> Vec<String> {
    if visited.contains(package_name) {
        // Avoid infinite recursion caused by circular dependencies
        return vec![];
    }
    visited.insert(package_name.to_string());

    let mut result = Vec::new();
    for dep_name in &package.depends {
        if let Some(package) = package_map.get(dep_name) {
            let recursive_dependencies =
                get_package_dependencies(package_map, dep_name, package, visited);
            result.extend(recursive_dependencies);
        }
    }

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
        .into_iter()
        .map(|i| items_in_order[i].clone())
        .collect::<Vec<String>>();

    filesystem::save_file(config_path, local_config).context("Writing local config to file")
}
