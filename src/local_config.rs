use crate::args::Options;
use crate::config::{load_global_config, load_local_config, GlobalConfig, LocalConfig, Package};
use crate::filesystem;
use anyhow::Context;
use anyhow::Result;
use dialoguer::MultiSelect;
use std::collections::{BTreeMap, HashSet};

/// Returns true if an error was printed
pub fn config(opt: &Options) -> Result<bool> {
    let global_config: GlobalConfig = load_global_config(&opt.global_config)?;

    let mut visited = HashSet::new();
    let mut packages = vec![];
    for package in global_config.packages.values() {
        let tree = get_package_tree(&global_config.packages, package, 0, &mut visited);
        debug!("Generated tree: {:#?}", tree);
        packages.extend(tree);
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
        for (pretty_name, package_name) in packages.iter() {
            multi_select.item_checked(pretty_name, enabled_packages.contains(package_name));
        }

        println!("Use space to select packages to enable, and enter to confirm");
        let selected_items = multi_select.interact_opt()?;
        trace!("Selected elements: {:?}", selected_items);

        match selected_items {
            Some(selected_items) => {
                modify_and_save(
                    opt,
                    &mut local_config,
                    packages.iter().map(|(_, value)| value).collect(),
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
                    .map(|(key, _)| key)
                    .collect::<Vec<&String>>()
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

/// Pretty Name, Package Name
type PackageNames = (String, String);

fn get_package_tree<'a>(
    package_map: &'a BTreeMap<String, Package>,
    package: &'a Package,
    indent: usize,
    visited: &mut HashSet<&'a Package>,
) -> Vec<PackageNames> {
    let package_name = package_map.iter().find(|(_, p)| *p == package).unwrap().0;
    trace!("Computing tree for package {}", package_name);

    let mut result = vec![];
    if visited.contains(package) {
        debug!("Already visited package {package_name}!");
        result.push((
            format!("{:indent$}(Cyclic Dependency)", "", indent = indent),
            String::from("(Cyclic Dependency)"),
        ));
        return result;
    }
    visited.insert(package);

    let mut fancy_name = format!("{:indent$}{}", "", package_name, indent = indent);

    for dep_name in &package.depends {
        trace!("Computing tree for dependency {}", dep_name);

        // when running with depth = 0, we need to add extra padding to align the package name 2 spaces to the right of the parent
        // when running with depth > 0, we just add 2 spaces to the existing padding
        let indent = indent + 2 + if indent == 0 { 6 } else { 0 };

        let dep_package = package_map.get(dep_name).unwrap();
        let dep_result = get_package_tree(package_map, dep_package, indent, visited);
        trace!("Found dependency tree: {:#?}", dep_result);

        // add dependency tree to fancy package name
        fancy_name += "\n";
        fancy_name += &dep_result
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<String>>()
            .join("\n");
        trace!("New fancy_name: {}", fancy_name);
    }

    result.push((fancy_name.clone(), package_name.clone()));
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
