use toml::value::Table;
use filesystem;

use std::collections::BTreeMap;
use std::path::Path;
use std::process;

/// Panics if table's values aren't strings
fn pretty_print(table: &Table) -> String {
    let mut output = String::new();
    for pair in table {
        output.push_str(pair.0);
        output.push_str(" = ");
        output.push_str(pair.1.as_str().unwrap());
        output.push('\n');
    }
    output.pop(); // Last \n
    output
}

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
fn parse_configuration_table(
    table: Table,
) -> BTreeMap<String, (BTreeMap<String, String>, BTreeMap<String, String>)> {
    table
        .iter()
        .map(|(package_key, package_value)| {
            let mut files = BTreeMap::new();
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

            let mut variables = BTreeMap::new();
            if let Some(variables_table) = package_value.get("variables").and_then(|f| f.as_table())
            {
                for (from, to) in variables_table.iter() {
                    if to.is_str() {
                        variables.insert(from.to_string(), to.as_str().unwrap().to_string());
                    } else if to.is_bool() && to.as_bool().unwrap() {
                        continue;
                    } else {
                        warn!(
                            "In package {} variable {}, value {} is invalid.",
                            package_key, from, to
                        );
                    }
                }
            }

            (package_key.to_string(), (files, variables))
        })
        .collect()
}

/// Returns a tuple of (files, variables) if successful
pub fn load_configuration(
    local_config: &Path,
    global_config: &Path,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>), String> {
    let global: Table = filesystem::load_file(global_config).map_err(|e| format!("global: {}", e))?;
    debug!("Global: {:?}", global);

    let local: Table = filesystem::load_file(local_config).map_err(|e| format!("local: {}", e))?;
    debug!("Local: {:?}", local);

    let packages = local
        .get("packages")
        .and_then(|v| v.as_array())
        .and_then(|v| {
            v.iter()
                .map(|i| i.as_str())
                .collect::<Option<Vec<&str>>>()
        })
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
    let configuration = {
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
    debug!("Final configuration: {:?}", configuration);

    Ok(configuration)
}

