use std::collections::{BTreeMap, BTreeSet};
use std::iter::FromIterator;
use std::path::Path;

use anyhow::Result;
use anyhow::{Context, Error};
use crossterm::style::{style, Color, Stylize};
use dialoguer::{MultiSelectPlus, MultiSelectPlusItem, MultiSelectPlusStatus, SelectCallback};

use crate::args::Options;
use crate::config::{load_global_config, load_local_config, LocalConfig, Package};
use crate::filesystem;

const DEPENDENCY: MultiSelectPlusStatus = MultiSelectPlusStatus {
    checked: false,
    symbol: "-",
};

/// Returns true if an error was printed
pub fn config(opt: &Options) -> Result<bool> {
    let global_config = load_global_config(&opt.global_config)?;
    let local_config = load_local_config(&opt.local_config)?;

    let packages = global_config
        .packages
        .iter()
        .map(|(name, package)| {
            let mut dependencies = BTreeSet::new();
            visit_recursively(&global_config.packages, name, package, &mut dependencies);
            dependencies.remove(name);
            (name.clone(), dependencies)
        })
        .collect::<BTreeMap<String, BTreeSet<String>>>();

    trace!("Available packages: {:?}", packages);

    let enabled_packages = if let Some(ref local_config) = local_config {
        BTreeSet::from_iter(local_config.packages.iter().cloned())
    } else {
        debug!(
            "No local configuration file found at {}",
            opt.local_config.display()
        );

        BTreeSet::new()
    };

    let multi_select = MultiSelectPlus::new().with_select_callback(select_callback(&packages));

    let selected_items = prompt(multi_select, &packages, &enabled_packages)?;
    trace!("Selected elements: {:?}", selected_items);

    write_selected_elements(
        &opt.local_config,
        local_config.unwrap_or_default(),
        selected_items,
        &packages,
    )?;

    Ok(false)
}

fn select_callback(packages: &BTreeMap<String, BTreeSet<String>>) -> Box<SelectCallback> {
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
    packages: &BTreeMap<String, BTreeSet<String>>,
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
    packages: &BTreeMap<String, BTreeSet<String>>,
    enabled_packages: &BTreeSet<String>,
) -> bool {
    packages
        .iter()
        .filter(|(key, _)| enabled_packages.contains(*key))
        .any(|(_, dependencies)| dependencies.contains(package_name))
}

fn write_selected_elements(
    config_path: &Path,
    mut local_config: LocalConfig,
    selected_elements: Option<Vec<usize>>,
    packages: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), Error> {
    match selected_elements {
        Some(selected_elements) => modify_and_save(
            config_path,
            &mut local_config,
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

fn format_package(package_name: &String, dependencies: &BTreeSet<String>) -> String {
    let dependencies: Vec<&str> = dependencies.iter().map(|s| s.as_str()).collect();
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

fn visit_recursively(
    package_map: &BTreeMap<String, Package>,
    package_name: &str,
    package: &Package,
    visited: &mut BTreeSet<String>,
) {
    if visited.contains(package_name) {
        // Avoid infinite recursion caused by circular dependencies
        return;
    }
    visited.insert(package_name.to_string());

    for dep_name in &package.depends {
        if let Some(package) = package_map.get(dep_name) {
            visit_recursively(package_map, dep_name, package, visited);
        }
    }
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
