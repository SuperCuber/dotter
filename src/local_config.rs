use anyhow::Context;
use anyhow::Result;
use dialoguer::MultiSelect;
use crate::args::Options;
use crate::filesystem;
use crate::config::{Configuration, GlobalConfig, load_global_config, load_local_config, LocalConfig, merge_configuration_files};

/// Returns true if an error was printed
pub fn config(opt: &Options) -> Result<bool> {
    let global_config: GlobalConfig = load_global_config(&opt.global_config)?;

    let mut multi_select = MultiSelect::new();

    return if opt.local_config.exists() {
        debug!("Local configuration file found at {}", opt.local_config.display());

        let mut local_config: LocalConfig = load_local_config(&opt.local_config)?;

        // this "config" variable will only contain the ENABLED packages
        let config: Configuration = merge_configuration_files(global_config.clone(), local_config.clone(), None)?;
        let enabled_packages = config.packages;

        // all packages, including the ones that are disabled
        let packages: Vec<&String> = global_config.packages.keys().collect();
        for name in &packages {
            multi_select.item_checked(name, enabled_packages.contains_key(*name));
        }

        println!("Use space to select packages to enable, and enter to confirm");
        let selected_items = multi_select.interact_opt()?;
        trace!("Selected elements: {:?}", selected_items);

        match selected_items {
            Some(selected_items) => {
                modify_and_save(opt, &mut local_config, packages, selected_items)?;
            }
            None => {
                // user pressed "Esc" or "q" to quit
                println!("Aborting.");
            }
        }
        Ok(false)
    } else {
        debug!("No local configuration file found at {}", opt.local_config.display());
        let packages: Vec<&String> = global_config.packages.keys().collect();
        trace!("Available packages: {:?}", packages);

        println!("Use space to select packages to enable, and enter to confirm");
        let selected_elements = multi_select.with_prompt("Select packages to install")
            .items(&packages)
            .interact_opt()?;
        trace!("Selected elements: {:?}", selected_elements);

        match selected_elements {
            Some(selected_elements) if selected_elements.is_empty() => {
                println!("No packages selected, writing empty configuration");
                filesystem::save_file(opt.local_config.as_path(), LocalConfig::empty_config())
                    .context("Writing empty configuration")?;
            }
            Some(selected_elements) => {
                modify_and_save(opt, &mut LocalConfig::empty_config(), packages, selected_elements)?;
            },
            None => {
                // user pressed "Esc" or "q" to quit
                println!("Aborting.");
            }
        }
        Ok(false)
    }
}

fn modify_and_save(opt: &Options, local_config: &mut LocalConfig, items_in_order: Vec<&String>, selected_items: Vec<usize>) -> Result<()> {
    println!("Writing configuration to {}", opt.local_config.display());
    trace!("Selected indexes: {:?} of {:?}", selected_items, items_in_order);
    local_config.packages = selected_items.iter().map(|i| items_in_order[*i].clone()).collect();
    filesystem::save_file(&opt.local_config, local_config)
        .context("Writing local config to file")
}
