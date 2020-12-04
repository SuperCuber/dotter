use anyhow::{Context, Result};

use watchexec;

use super::display_error;
use args::{Options, WatchedAction};
use deploy;
use difference;

struct WatchDeployHandler(Options);
struct WatchDiffHandler(Options);

impl watchexec::Handler for WatchDeployHandler {
    fn on_manual(&self) -> watchexec::error::Result<bool> {
        println!("[Dotter] Deploying...");
        if let Err(e) = deploy::deploy(&self.0) {
            display_error(e);
        }
        Ok(true)
    }

    fn on_update(&self, _: &[watchexec::pathop::PathOp]) -> watchexec::error::Result<bool> {
        self.on_manual()
    }

    fn args(&self) -> watchexec::Args {
        watchexec::cli::ArgsBuilder::default()
            .cmd(vec!["".into()])
            .paths(vec![".".into()])
            .build()
            .expect("valid watchexec args")
    }
}

impl watchexec::Handler for WatchDiffHandler {
    fn on_manual(&self) -> watchexec::error::Result<bool> {
        println!("[Dotter] Diffing...");
        if let Err(e) = difference::diff(&self.0) {
            display_error(e);
        }
        Ok(true)
    }

    fn on_update(&self, _: &[watchexec::pathop::PathOp]) -> watchexec::error::Result<bool> {
        self.on_manual()
    }

    fn args(&self) -> watchexec::Args {
        watchexec::cli::ArgsBuilder::default()
            .cmd(vec!["".into()])
            .paths(vec![".".into()])
            .build()
            .expect("valid watchexec args")
    }
}

pub(crate) fn watch(opt: Options, action: WatchedAction) -> Result<()> {
    match action {
        WatchedAction::Deploy => {
            watchexec::watch(&WatchDeployHandler(opt)).context("run watch deploy")?;
        }
        WatchedAction::Diff => {
            watchexec::watch(&WatchDiffHandler(opt)).context("run watch diff")?;
        }
    }

    Ok(())
}
