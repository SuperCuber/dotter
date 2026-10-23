use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail, ensure};

use crate::config::Cache;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetachState {
    Missing,
    File,
    Symlink(PathBuf),
}

/// Owns a temporary file until it is persisted; failures clean up automatically.
#[derive(Default)]
pub struct PreparedFile {
    path: Option<PathBuf>,
}

impl PreparedFile {
    fn create(target: &Path) -> Result<(Self, File)> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = target.parent().context("target has no parent")?;
        for _ in 0..1024 {
            let path = parent.join(format!(
                ".dotter-detach-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((Self { path: Some(path) }, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("create private temporary file"),
            }
        }
        bail!("could not allocate a unique temporary file")
    }

    pub(super) fn persist(&mut self, target: &Path) -> Result<()> {
        let path = self.path.as_ref().context("file was not prepared")?;
        fs::rename(path, target).context("replace target with prepared file")?;
        self.path = None;
        Ok(())
    }
}

impl Drop for PreparedFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            // Windows cannot remove a read-only temporary copy without clearing
            // the attribute. Never change attributes on a substituted symlink.
            #[cfg(windows)]
            if let Ok(metadata) = fs::symlink_metadata(path) {
                if metadata.is_file() && metadata.permissions().readonly() {
                    let mut permissions = metadata.permissions();
                    permissions.set_readonly(false);
                    let _ = fs::set_permissions(path, permissions);
                }
            }
            if let Err(error) = fs::remove_file(path) {
                warn!("Could not remove temporary file {path:?}: {error}");
            }
        }
    }
}

#[cfg(unix)]
fn ensure_owned(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        metadata.uid() == unsafe { libc::geteuid() },
        "file belongs to another user; detach does not elevate privileges"
    );
    Ok(())
}

#[cfg(windows)]
fn ensure_owned(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

pub fn validate_detach_cache(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("inspect cache file")?;
    ensure!(metadata.is_file(), "cache must be a regular file");
    #[cfg(windows)]
    ensure!(
        !metadata.permissions().readonly(),
        "read-only cache cannot be replaced"
    );
    ensure_owned(&metadata)
}

pub(super) fn inspect(source: Option<&Path>, target: &Path) -> Result<DetachState> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(DetachState::Missing),
        Err(error) => return Err(error).context("inspect target"),
    };
    if metadata.is_file() {
        return Ok(DetachState::File);
    }
    ensure!(
        metadata.is_symlink(),
        "target is not a regular file or file symlink; directories are not supported"
    );
    let source = source.context("template target is a symlink, not a regular file")?;
    ensure_owned(&metadata)?;
    ensure!(
        fs::metadata(source).context("inspect source")?.is_file(),
        "source is not a regular file; directories are not supported"
    );
    let resolved_source = fs::canonicalize(source).context("resolve expected source")?;
    ensure!(
        fs::canonicalize(target).context("resolve target symlink")? == resolved_source,
        "target symlink no longer points to the expected source"
    );
    File::open(source).context("check source is readable")?;
    Ok(DetachState::Symlink(resolved_source))
}

pub(super) fn prepare_copy(source: &Path, target: &Path) -> Result<PreparedFile> {
    let mut source = File::open(source).context("open source for copy")?;
    let metadata = source.metadata().context("inspect opened source")?;
    ensure!(metadata.is_file(), "source is no longer a regular file");
    let (prepared, mut file) = PreparedFile::create(target)?;
    io::copy(&mut source, &mut file).context("copy source contents")?;
    file.set_permissions(metadata.permissions())
        .context("preserve source permissions")?;
    file.sync_all().context("flush prepared copy")?;
    Ok(prepared)
}

pub(super) fn prepare_cache(path: &Path, cache: &Cache) -> Result<PreparedFile> {
    let content = toml::to_string(cache).context("serialize cache checkpoint")?;
    let (prepared, mut file) = PreparedFile::create(path)?;
    file.write_all(content.as_bytes())
        .context("write cache checkpoint")?;
    file.set_permissions(
        fs::metadata(path)
            .context("inspect cache permissions")?
            .permissions(),
    )
    .context("preserve cache permissions")?;
    file.sync_all().context("flush cache checkpoint")?;
    Ok(prepared)
}
