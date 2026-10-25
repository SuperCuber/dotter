use anyhow::{Context, Result};
use watchexec::error::RuntimeError;
use watchexec::filter::Filterer;
use watchexec::sources::fs::Watcher;
use watchexec::{Config, Watchexec};
use watchexec_events::filekind::FileEventKind;
use watchexec_events::{Event, Priority, Tag};
use watchexec_filterer_globset::GlobsetFilterer;

use super::display_error;
use crate::args::Options;
use crate::deploy;

/// Discards events that don't modify anything, since deploying reads the repository's files and
/// would otherwise retrigger itself forever.
#[derive(Debug)]
struct ModificationFilterer(GlobsetFilterer);

impl Filterer for ModificationFilterer {
    fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
        let is_access = event
            .tags
            .iter()
            .any(|tag| matches!(tag, Tag::FileEventKind(FileEventKind::Access(_))));
        if is_access {
            return Ok(false);
        }
        self.0.check_event(event, priority)
    }
}

fn deploy_and_report(opt: &Options, changes_detected: bool) {
    if crate::ui::fancy() {
        let message = if changes_detected {
            "Changes detected, deploying..."
        } else {
            "Deploying..."
        };
        anstream::println!("{} {message}", crate::ui::paint(crate::ui::HINT, "::"));
    } else {
        println!("[Dotter] Deploying...");
    }
    if let Err(e) = deploy::deploy(opt) {
        display_error(e);
    }
}

pub(crate) async fn watch(opt: Options) -> Result<()> {
    let config = Config::default();

    config.file_watcher(Watcher::Native);
    config.pathset(["."]);

    let filter = GlobsetFilterer::new(
        std::env::current_dir()?,
        vec![],
        // Directories are ignored with both `dir` and `dir/**`: a `dir/` pattern alone doesn't
        // reliably exclude the files inside it.
        vec![
            (format!("{}", opt.cache_directory.display()), None),
            (format!("{}/**", opt.cache_directory.display()), None),
            (format!("{}", opt.cache_file.display()), None),
            (".git".to_string(), None),
            (".git/**".to_string(), None),
            ("DOTTER_SYMLINK_TEST".to_string(), None),
        ],
        vec![],
        vec![],
        vec![],
    )
    .await?;

    config.filterer(ModificationFilterer(filter));

    deploy_and_report(&opt, false);

    config.on_action(move |mut action| {
        if action.signals().next().is_some() {
            action.quit();
            return action;
        }

        debug!("Changes detected in watched files.");
        trace!("Changed files: {:#?}", action.paths().collect::<Vec<_>>());

        deploy_and_report(&opt, true);

        action
    });

    config.on_error(move |e| {
        log::error!("Watcher error: {e:#?}");
    });

    let we = Watchexec::with_config(config)?;
    we.main().await.context("run watchexec main loop")??;
    Ok(())
}
