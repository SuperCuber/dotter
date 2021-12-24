use anyhow::{Context, Result};

use super::display_error;
use crate::args::Options;
use crate::deploy;

struct WatchDeployHandler(Options);

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

    fn args(&self) -> watchexec::config::Config {
        watchexec::config::ConfigBuilder::default()
            .cmd(vec!["".into()])
            .filters(vec!["*".into(), ".*".into()])
            .ignores(vec![
                ".git".into(),
                self.0.cache_file.to_string_lossy().into(),
                self.0.cache_directory.to_string_lossy().into(),
                "DOTTER_SYMLINK_TEST".into(),
            ])
            .paths(vec![".".into()])
            .build()
            .expect("valid watchexec args")
    }
}

pub(crate) fn watch(opt: Options) -> Result<()> {
    watchexec::watch(&WatchDeployHandler(opt)).context("run watch deploy")?;

    Ok(())
}
