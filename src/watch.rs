use anyhow::{Context, Result};
use watchexec::sources::fs::Watcher;
use watchexec::{Config, Watchexec};
use watchexec_filterer_tagged::{Filter, Matcher, Op, Pattern, TaggedFilterer};

use super::display_error;
use crate::args::Options;
use crate::deploy;

pub(crate) async fn watch(opt: Options) -> Result<()> {
    let config = Config::default();

    config.file_watcher(Watcher::Native);
    config.pathset(["."]);

    let filter = TaggedFilterer::new(".".into(), std::env::current_dir()?)
        .await
        .unwrap();
    filter
        .add_filters(&[
            Filter {
                in_path: None,
                on: Matcher::Path,
                op: Op::NotGlob,
                pat: Pattern::Glob(format!("{}/", opt.cache_directory.display())),
                negate: false,
            },
            Filter {
                in_path: None,
                on: Matcher::Path,
                op: Op::NotGlob,
                pat: Pattern::Glob(opt.cache_file.to_string_lossy().into()),
                negate: false,
            },
            Filter {
                in_path: None,
                on: Matcher::Path,
                op: Op::NotGlob,
                pat: Pattern::Glob(".git/".into()),
                negate: false,
            },
            Filter {
                in_path: None,
                on: Matcher::Path,
                op: Op::NotEqual,
                pat: Pattern::Exact("DOTTER_SYMLINK_TEST".into()),
                negate: false,
            },
        ])
        .await?;
    config.filterer(filter);

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
