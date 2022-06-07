use std::convert::Infallible;
use std::sync::Arc;

use anyhow::{Context, Result};
use log::Level;
use watchexec::action::{Action, Outcome};
use watchexec::config::{InitConfig, RuntimeConfig};
use watchexec::filter::tagged::{Filter, Matcher, Op, Pattern, TaggedFilterer};
use watchexec::fs::Watcher;
use watchexec::handler::SyncFnHandler;
use watchexec::Watchexec;

use super::display_error;
use crate::args::Options;
use crate::deploy;

pub(crate) async fn watch(opt: Options) -> Result<()> {
    let mut init = InitConfig::default();
    let mut errors = false;
    init.on_error(SyncFnHandler::from(move |e| {
        if !errors && !log::log_enabled!(Level::Debug) {
            log::warn!("Watcher produced errors. Re-run with -vv to see them.");
            errors = true;
        }
        log::debug!("Watcher error: {e:#?}");
        Ok::<(), Infallible>(())
    }));

    let mut runtime = RuntimeConfig::default();
    runtime.file_watcher(Watcher::Native);
    runtime.pathset(["."]);

    let filter = TaggedFilterer::new(".", std::env::current_dir()?).unwrap();
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
    runtime.filterer(Arc::new(filter));

    runtime.on_action(move |action: Action| {
        let opt = opt.clone();
        async move {
            if action.events.iter().any(|e| e.signals().next().is_some()) {
                action.outcome(Outcome::Exit);
                return Ok(());
            }

            println!("[Dotter] Deploying...");
            if let Err(e) = deploy::deploy(&opt) {
                display_error(e);
            }

            action.outcome(Outcome::if_running(Outcome::DoNothing, Outcome::Start));

            Ok::<(), Infallible>(())
        }
    });

    let we = Watchexec::new(init, runtime.clone())?;
    we.main().await.context("run watchexec main loop")??;
    Ok(())
}
