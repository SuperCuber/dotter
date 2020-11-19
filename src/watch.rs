use anyhow::{Context, Result};

use watchexec;

use super::display_error;
use args::Options;
use deploy;

struct DotterWatchHandler(Options);

impl watchexec::Handler for DotterWatchHandler {
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

pub(crate) fn watch(opt: Options) -> Result<()> {
    watchexec::watch(&DotterWatchHandler(opt)).context("run watchexec")
}
