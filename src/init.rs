use anyhow::{Context, Result};

use std::collections::BTreeMap;

use crate::args::Options;
use crate::config;
use crate::filesystem::save_file;

pub fn init(opt: Options) -> Result<()> {
    info!("Looking for existing configuration...");
    if opt.global_config.exists() {
        if opt.force {
            warn!("Configuration already exists. Overwriting because of --force");
        } else {
            anyhow::bail!("Configuration already exists. Use --force to overwrite.");
        }
    } else {
        info!("No existing configuration.");
    }

    debug!("Reading files from current directory...");
    let mut files = Vec::new();
    for file in std::fs::read_dir(".").context("read contents of current directory")? {
        let file = file.context("get next file")?;
        let name = file
            .file_name()
            .into_string()
            .map_err(|f| anyhow::anyhow!("filename {:?} is not valid unicode", f))?;
        if name.starts_with('.') {
            debug!("Ignored file {:?}", name);
            continue;
        }
        files.push(name);
    }
    trace!("Files: {:#?}", files);

    config::save_dummy_config(files, &opt.local_config, &opt.global_config)
        .context("save dummy config")?;

    debug!("Emptying cache...");
    save_file(
        &opt.cache_file,
        config::Cache {
            symlinks: BTreeMap::default(),
            copies: BTreeMap::default(),
            templates: BTreeMap::default(),
        },
    )
    .context("save empty cache file")?;
    match std::fs::remove_dir_all(opt.cache_directory) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
    .context("remove cache directory")?;

    Ok(())
}
