use std::path::PathBuf;

use anyhow::{Context, Result};
use watchexec::filter::Filterer;
use watchexec::{Config, Watchexec};

use super::display_error;
use crate::args::Options;
use crate::deploy;

#[derive(Debug)]
struct MyFilterer {
    cache_directory: PathBuf,
    cache_file: PathBuf,
}

impl Filterer for MyFilterer {
    fn check_event(
        &self,
        event: &watchexec_events::Event,
        _priority: watchexec_events::Priority,
    ) -> Result<bool, watchexec::error::RuntimeError> {
        let path = 'block: {
            for tag in &event.tags {
                match tag {
                    watchexec_events::Tag::Path { path, .. } => break 'block Some(path),
                    _ => {}
                }
            }
            break 'block None;
        };
        let Some(path) = path else { return Ok(false) };

        let ans = !path.starts_with(&self.cache_directory.canonicalize().unwrap())
            && path != &self.cache_file.canonicalize().unwrap()
            && path != &PathBuf::from(".").canonicalize().unwrap().join(".git")
            && path
                .file_name()
                .map(|s| s != "DOTTER_SYMLINK_TEST")
                .unwrap_or(true);

        if ans {
            dbg!(path);
        }

        Ok(ans)
    }
}

pub(crate) async fn watch(opt: Options) -> Result<()> {
    let config = Config::default();
    config.filterer(MyFilterer {
        cache_directory: opt.cache_directory.clone(),
        cache_file: opt.cache_file.clone(),
    });
    config.pathset(["."]);

    config.on_action(move |mut action| {
        let opt = opt.clone();
        if action.signals().next().is_some() {
            action.quit();
            return action;
        }

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
