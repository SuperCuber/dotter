use anyhow::{Context, Result};
use thiserror::Error;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use crate::config::UnixUser;

// === Serialize/deserialize files ===

#[derive(Error, Debug)]
pub enum FileLoadError {
    #[error("open file")]
    Open(#[source] io::Error),

    #[error("read opened file")]
    Read(#[source] io::Error),

    #[error("parse file")]
    Parse(#[source] toml::de::Error),
}

pub fn load_file<T>(filename: &Path) -> Result<T, FileLoadError>
where
    T: DeserializeOwned,
{
    let mut buf = String::new();
    let mut f = File::open(filename).map_err(FileLoadError::Open)?;
    f.read_to_string(&mut buf).map_err(FileLoadError::Read)?;
    toml::from_str::<T>(&buf).map_err(FileLoadError::Parse)
}

#[derive(Error, Debug)]
pub enum FileSaveError {
    #[error("write file")]
    Write(#[source] io::Error),

    #[error("serialize data")]
    Serialize(
        #[from]
        #[source]
        toml::ser::Error,
    ),
}

pub fn save_file<T>(filename: &Path, data: T) -> Result<(), FileSaveError>
where
    T: Serialize,
{
    let data = toml::to_string(&data)?;
    fs::write(filename, &data).map_err(FileSaveError::Write)?;
    Ok(())
}

// === Mockable filesystem ===

#[mockall::automock]
pub trait Filesystem {
    /// Check state of expected symlink on disk
    fn compare_symlink(&mut self, source: &Path, link: &Path) -> Result<SymlinkComparison>;

    /// Check state of expected symbolic link on disk
    fn compare_template(&mut self, target: &Path, cache: &Path) -> Result<TemplateComparison>;

    /// Removes a file or folder, elevating privileges if needed
    fn remove_file(&mut self, path: &Path) -> Result<()>;

    /// Read contents of file into a string
    fn read_to_string(&mut self, path: &Path) -> Result<String>;

    /// Write string to file, without elevating privileges
    fn write(&mut self, path: &Path, content: String) -> Result<()>;

    /// Delete parents of target file if they're empty
    fn delete_parents(&mut self, path: &Path) -> Result<()>;

    /// Makes a symlink owned by the selected user, elevating privileges as needed
    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// Create directory (and its parents) owned by the selected user,
    /// elevating privileges as needed
    fn create_dir_all(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// Copy readable file to target existing location.
    /// Target file will be owned by the selected user. Privileges elevated as needed.
    fn copy_file(&mut self, source: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// If owner.is_some, elevates privileges and sets file to that owner
    /// If owner.is_none, ensures file is owned by the current user (elevating privileges if needed)
    fn set_owner(&mut self, file: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// Copy file mode, elevating privileges as needed. (Does not change owner)
    fn copy_permissions(
        &mut self,
        source: &Path,
        target: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<()>;
}

// == Windows Filesystem ==

#[cfg(windows)]
pub struct RealFilesystem {
    interactive: bool,
}

#[cfg(windows)]
impl RealFilesystem {
    pub fn new(interactive: bool) -> RealFilesystem {
        RealFilesystem { interactive }
    }
}

#[cfg(windows)]
impl Filesystem for RealFilesystem {
    fn compare_symlink(&mut self, source: &Path, link: &Path) -> Result<SymlinkComparison> {
        compare_symlink(source, link)
    }

    fn compare_template(&mut self, target: &Path, cache: &Path) -> Result<TemplateComparison> {
        compare_template(target, cache)
    }

    fn remove_file(&mut self, path: &Path) -> Result<()> {
        // TODO: test if this removes a folder too
        std::fs::remove_file(path).context("remove file")
    }

    fn read_to_string(&mut self, path: &Path) -> Result<String> {
        fs::read_to_string(path).context("read from file")
    }

    fn write(&mut self, path: &Path, content: String) -> Result<()> {
        fs::write(path, content).context("write to file")
    }

    fn delete_parents(&mut self, path: &Path) -> Result<()> {
        let mut path = path.parent().context("get parent")?;
        while path.is_dir()
            && path
                .read_dir()
                .context("read the contents of parent directory")?
                .next()
                .is_none()
        {
            if !self.interactive
                || ask_boolean(&format!(
                    "Directory at {:?} is now empty. Delete [y/N]? ",
                    path
                ))
            {
                std::fs::remove_dir(path).context(format!("remove directory {:?}", path))?;
            }
            path = path.parent().context(format!("get parent of {:?}", path))?;
        }
        Ok(())
    }

    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        use std::os::windows::fs;

        if let Some(owner) = owner {
            warn!(
                "Ignoring `owner`={:?} when creating symlink {:?} -> {:?}",
                owner, link, target
            );
        }
        Ok(fs::symlink_file(
            real_path(target).context("get real path of source file")?,
            link,
        )
        .context("create symlink")?)
    }

    fn create_dir_all(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if let Some(owner) = owner {
            warn!(
                "Ignoring `owner`={:?} when creating directory {:?}",
                owner, path
            );
        }
        std::fs::create_dir_all(path).context("create directories")
    }

    fn copy_file(&mut self, source: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if let Some(owner) = owner {
            warn!(
                "Ignoring `owner`={:?} when copying {:?} -> {:?}",
                owner, source, target
            );
        }
        std::fs::copy(source, target).context("copy file")?;
        Ok(())
    }

    fn set_owner(&mut self, file: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if owner.is_some() {
            warn!("ignoring `owner` field on file {:?}", file);
        }
        Ok(())
    }

    fn copy_permissions(
        &mut self,
        source: &Path,
        target: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<()> {
        if let Some(owner) = owner {
            warn!(
                "Ignoring `owner`={:?} when copying permissions {:?} -> {:?}",
                owner, source, target
            );
        }
        std::fs::set_permissions(
            target,
            source
                .metadata()
                .context("get source metadata")?
                .permissions(),
        )
        .context("set target permissions")
    }
}

// == Unix Filesystem ==

#[cfg(unix)]
pub struct RealFilesystem {
    interactive: bool,
    sudo_occurred: bool,
}

#[cfg(unix)]
impl RealFilesystem {
    pub fn new(interactive: bool) -> RealFilesystem {
        RealFilesystem {
            sudo_occurred: false,
            interactive,
        }
    }

    fn warn_sudo(&mut self, reason: &str) {
        if !self.sudo_occurred {
            warn!("Elevating permissions to {}...", reason);
            self.sudo_occurred = true;
        }
    }

    fn is_owned_by_user(&self, path: &Path) -> Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let file_uid = path.metadata().context("get file metadata")?.uid();
        let process_uid = std::path::PathBuf::from("/proc/self")
            .metadata()
            .context("get metadata of /proc/self")?
            .uid();
        Ok(file_uid == process_uid)
    }
}

#[cfg(unix)]
impl Filesystem for RealFilesystem {
    fn compare_symlink(&mut self, source: &Path, link: &Path) -> Result<SymlinkComparison> {
        compare_symlink(source, link)
    }

    fn compare_template(&mut self, target: &Path, cache: &Path) -> Result<TemplateComparison> {
        compare_template(target, cache)
    }

    fn remove_file(&mut self, path: &Path) -> Result<()> {
        let metadata = path.metadata().context("get metadata")?;
        let result = if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                debug!("Removing file {:?} as root", path);
                self.warn_sudo("remove a file (no permission as current user)");
                let success = std::process::Command::new("sudo")
                    .arg("rm")
                    .arg("-r")
                    .arg(path)
                    .spawn()
                    .context("spawn sudo rm command")?
                    .wait()
                    .context("wait for sudo rm command")?
                    .success();

                anyhow::ensure!(success, "sudo rm command failed");
                Ok(())
            }
            Err(e) => Err(e).context("remove file"),
        }
    }

    fn read_to_string(&mut self, path: &Path) -> Result<String> {
        fs::read_to_string(path).context("read from file")
    }

    fn write(&mut self, path: &Path, content: String) -> Result<()> {
        fs::write(path, content).context("write to file")
    }

    fn delete_parents(&mut self, path: &Path) -> Result<()> {
        let mut path = path.parent().context("get parent")?;
        while path.is_dir()
            && path
                .read_dir()
                .context("read the contents of parent directory")?
                .next()
                .is_none()
        {
            if !self.interactive
                || ask_boolean(&format!(
                    "Directory at {:?} is now empty. Delete [y/N]? ",
                    path
                ))
            {
                match std::fs::remove_dir(path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        let success = std::process::Command::new("sudo")
                            .arg("rmdir")
                            .arg(path)
                            .spawn()
                            .context("spawn sudo rmdir")?
                            .wait()
                            .context("wait for sudo rmdir")?
                            .success();

                        anyhow::ensure!(success, "sudo rmdir failed");
                        Ok(())
                    }
                    Err(e) => Err(e).context("remove dir"),
                }
            }
            path = path.parent().context(format!("get parent of {:?}", path))?;
        }
        Ok(())
    }

    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        use std::os::unix::fs;

        if let Some(owner) = owner {
            debug!(
                "Creating symlink {:?} -> {:?} from user {:?}",
                link, target, owner
            );
            let success = std::process::Command::new("sudo")
                .arg("-u")
                .arg(owner.as_sudo_arg())
                .arg("ln")
                .arg("-s")
                .arg(real_path(target).context("get real path of source file")?)
                .arg(link)
                .spawn()
                .context("spawn sudo ln")?
                .wait()
                .context("wait for sudo ln")?
                .success();

            anyhow::ensure!(success, "sudo ln failed");
        } else {
            debug!(
                "Creating symlink {:?} -> {:?} as current user...",
                link, target
            );
            fs::symlink(
                real_path(target).context("get real path of source file")?,
                link,
            )
            .context("create symlink")?;
        }
        Ok(())
    }

    fn create_dir_all(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if let Some(owner) = owner {
            debug!("Creating directory {:?} from user {:?}...", path, owner);
            let success = std::process::Command::new("sudo")
                .arg("-u")
                .arg(owner.as_sudo_arg())
                .arg("mkdir")
                .arg("-p")
                .arg(path)
                .spawn()
                .context("spawn sudo mkdir")?
                .wait()
                .context("wait for sudo mkdir")?
                .success();

            anyhow::ensure!(success, "sudo mkdir failed");
        } else {
            debug!("Creating directory {:?} as current user...", path);
            std::fs::create_dir_all(path).context("create directories")?;
        }
        Ok(())
    }

    fn copy_file(&mut self, source: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if let Some(owner) = owner {
            debug!("Copying {:?} -> {:?} as user {:?}", source, target, owner);
            let contents = std::fs::read_to_string(source).context("read file contents")?;
            let mut child = std::process::Command::new("sudo")
                .arg("-u")
                .arg(owner.as_sudo_arg())
                .arg("tee")
                .arg(target)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .context("spawn sudo tee")?;

            // At this point we should've gone through another sudo at the mkdir step already,
            // so sudo will not ask for the password
            child
                .stdin
                .as_ref()
                .expect("has stdin")
                .write_all(contents.as_bytes())
                .context("give input to tee")?;

            let success = child.wait().context("wait for sudo tee")?.success();

            anyhow::ensure!(success, "sudo tee failed");
        } else {
            debug!("Copying {:?} -> {:?} as current user", source, target);
            std::fs::copy(source, target).context("copy file")?;
        }

        Ok(())
    }

    fn set_owner(&mut self, file: &Path, owner: &Option<UnixUser>) -> Result<()> {
        if self
            .is_owned_by_user(file)
            .context("detect if file is owned by the current user")?
            && owner.is_none()
        {
            // Nothing to do, no need to elevate
            return Ok(());
        }

        let owner = owner.clone().unwrap_or(UnixUser::Name(
            std::env::var("USER").context("get USER env var")?,
        ));
        debug!("Setting owner of {:?} to {:?}...", file, owner);

        let success = std::process::Command::new("sudo")
            .arg("chown")
            .arg(owner.as_chown_arg())
            .arg("-h") // no-dereference
            .arg(file)
            .spawn()
            .context("spawn sudo chown command")?
            .wait()
            .context("wait for sudo chown command")?
            .success();

        anyhow::ensure!(success, "sudo chown command failed");
        Ok(())
    }

    fn copy_permissions(
        &mut self,
        source: &Path,
        target: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<()> {
        if let Some(owner) = owner {
            debug!(
                "Copying permissions {:?} -> {:?} as user {:?}",
                source, target, owner
            );
            let success = std::process::Command::new("sudo")
                .arg("chmod")
                .arg("--reference")
                .arg(source)
                .arg(target)
                .spawn()
                .context("spawn sudo chmod command")?
                .wait()
                .context("wait for sudo chmod command")?
                .success();

            anyhow::ensure!(success, "sudo chmod failed");
        } else {
            debug!(
                "Copying permissions {:?} -> {:?} as current user",
                source, target
            );
            std::fs::set_permissions(
                target,
                source
                    .metadata()
                    .context("get source metadata")?
                    .permissions(),
            )
            .context("set target permissions")?;
        }
        Ok(())
    }
}

// === Comparisons ===

#[derive(Debug, PartialEq)]
pub enum SymlinkComparison {
    Identical,
    OnlySourceExists,
    OnlyTargetExists,
    TargetNotSymlink,
    Changed,
    BothMissing,
}

impl std::fmt::Display for SymlinkComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use self::SymlinkComparison::*;
        match self {
            Identical => "target points at source",
            OnlySourceExists => "target missing",
            OnlyTargetExists => "source is missing",
            TargetNotSymlink => "target isn't a symlink",
            Changed => "target doesn't point at source",
            BothMissing => "source and target are missing",
        }
        .fmt(f)
    }
}

pub fn compare_symlink(source: &Path, link: &Path) -> Result<SymlinkComparison> {
    let source = match real_path(source) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context("get canonical path of source")?,
    };

    let link_content = match fs::symlink_metadata(link) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Some(fs::read_link(link).context("read target of link")?)
        }
        Ok(_) => return Ok(SymlinkComparison::TargetNotSymlink),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context("read metadata of link")?,
    };

    Ok(match (source, link_content) {
        (Some(s), Some(l)) => {
            if s == l {
                SymlinkComparison::Identical
            } else {
                SymlinkComparison::Changed
            }
        }
        (None, Some(_)) => SymlinkComparison::OnlyTargetExists,
        (Some(_), None) => SymlinkComparison::OnlySourceExists,
        (None, None) => SymlinkComparison::BothMissing,
    })
}

#[derive(Debug, PartialEq)]
pub enum TemplateComparison {
    Identical,
    OnlyCacheExists,
    OnlyTargetExists,
    Changed,
    TargetNotRegularFile,
    BothMissing,
}

impl std::fmt::Display for TemplateComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use self::TemplateComparison::*;
        match self {
            Identical => "target and cache's contents are equal",
            OnlyCacheExists => "target doesn't exist",
            OnlyTargetExists => "cache doesn't exist",
            Changed => "target contents were changed",
            TargetNotRegularFile => "target is a symbolic link or directory",
            BothMissing => "cache and target are missing",
        }
        .fmt(f)
    }
}

pub fn compare_template(target: &Path, cache: &Path) -> Result<TemplateComparison> {
    if fs::read_link(target).is_ok() || fs::read_dir(target).is_ok() {
        return Ok(TemplateComparison::TargetNotRegularFile);
    }
    let target = match fs::read_to_string(target) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context("read content of target file")?,
    };

    let cache = match fs::read_to_string(cache) {
        Ok(c) => Some(c),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context("read contents of cache file")?,
    };

    Ok(match (target, cache) {
        (Some(t), Some(c)) => {
            if t == c {
                TemplateComparison::Identical
            } else {
                TemplateComparison::Changed
            }
        }
        (Some(_), None) => TemplateComparison::OnlyTargetExists,
        (None, Some(_)) => TemplateComparison::OnlyCacheExists,
        (None, None) => TemplateComparison::BothMissing,
    })
}

/// === Utility functions ===

pub fn real_path(path: &Path) -> Result<PathBuf, io::Error> {
    let path = std::fs::canonicalize(&path)?;
    Ok(platform_dunce(&path))
}

pub fn ask_boolean(prompt: &str) -> bool {
    let mut buf = String::from("a"); // enter the loop at least once
    while !(buf.to_lowercase().starts_with('y')
        || buf.to_lowercase().starts_with('n')
        || buf.is_empty())
    {
        eprintln!("{}", prompt);
        buf.clear();
        io::stdin()
            .read_line(&mut buf)
            .expect("Failed to read line from stdin");
    }

    // If empty defaults to no
    buf.to_lowercase().starts_with('y')
}

#[cfg(windows)]
pub fn symlinks_enabled(test_file_path: &Path) -> Result<bool> {
    use std::os::windows::fs;
    debug!(
        "Testing whether symlinks are enabled on path {:?}",
        test_file_path
    );
    let _ = std::fs::remove_file(&test_file_path);
    match fs::symlink_file("test.txt", &test_file_path) {
        Ok(()) => {
            std::fs::remove_file(&test_file_path)
                .context(format!("remove test file {:?}", test_file_path))?;
            Ok(true)
        }
        Err(e) => {
            // os error 1314: A required privilege is not held by the client.
            if e.raw_os_error() == Some(1314) {
                Ok(false)
            } else {
                Err(e).context(format!("create test symlink at {:?}", test_file_path))
            }
        }
    }
}

#[cfg(unix)]
pub fn symlinks_enabled(_test_file_path: &Path) -> Result<bool> {
    Ok(true)
}

#[cfg(windows)]
pub fn platform_dunce(path: &Path) -> PathBuf {
    dunce::simplified(&path).into()
}

#[cfg(unix)]
pub fn platform_dunce(path: &Path) -> PathBuf {
    path.into()
}
