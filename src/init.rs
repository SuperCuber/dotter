use anyhow::{Context, Result};

use args::Options;
use config;

pub fn init(opt: Options) -> Result<()> {
    info!("Looking for existing configuration...");
    let cwd = std::env::current_dir().context("get current directory")?;
    match config::load_configuration(&opt.local_config, &opt.global_config) {
        Ok(_) if !opt.force => {
            bail!("Configuration already exists. Use --force to overwrite.");
        }
        Ok(_) => {
            warn!("Configuration already exists. Overwriting because of --force");
        }
        Err(config::LoadConfigFailType::Find) => {
            info!("No existing configuration.");
            std::env::set_current_dir(cwd).context("restore current directory")?;
        }
        Err(e) => Err(e).context("load existing configuration")?,
    }

    debug!("Reading files from current directory...");
    let mut files = Vec::new();
    for file in std::fs::read_dir(".").context("read contents of current directory")? {
        let file = file.context("get next file")?;
        let name = file
            .file_name()
            .into_string()
            .map_err(|f| anyhow!("filename {:?} is not valid unicode", f))?;
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
    config::save_cache(
        &opt.cache_file,
        config::Cache {
            symlinks: Default::default(),
            templates: Default::default(),
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
