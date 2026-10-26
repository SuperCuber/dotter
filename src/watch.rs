use anyhow::{Context, Result};
use watchexec::sources::fs::Watcher;
use watchexec::{Config, Watchexec};
use watchexec_filterer_globset::GlobsetFilterer;

use super::display_error;
use crate::args::Options;
use crate::deploy;

pub(crate) async fn watch(opt: Options) -> Result<()> {
    let config = Config::default();

    config.file_watcher(Watcher::Native);
    config.pathset(["."]);

    let filter = GlobsetFilterer::new(
        std::env::current_dir()?,
        vec![
            (format!("!{}/", opt.cache_directory.display()), None),
            (format!("!{}", opt.cache_file.display()), None),
            ("!.git/".to_string(), None),
            ("!DOTTER_SYMLINK_TEST".to_string(), None),
        ],
        vec![],
        vec![],
        vec![],
        vec![], // Add the 6th argument (extensions)
    )
    .await?;

    config.filterer(filter);

    config.on_action(move |mut action| {
        if action.signals().next().is_some() {
            action.quit();
            return action;
        }

        debug!("Changes detected in watched files.");
        trace!("Changed files: {:#?}", action.paths().collect::<Vec<_>>());

        println!("[Dotter] Deploying...");
        if let Err(e) = deploy::deploy(&opt) {
            display_error(e);
        }

        action
    });

    config.on_error(move |e| {
        log::error!("Watcher error: {e:#?}");
    });

    let we = Watchexec::with_config(config)?;
    we.main().await.context("run watchexec main loop")??;
    Ok(())
}
