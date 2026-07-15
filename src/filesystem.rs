use anyhow::{Context, Result};

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;

use crate::config::UnixUser;

// === Serialize/deserialize files ===

/// Returns Ok(None) if file was not found, otherwise Ok(Some(data)) or Err
pub fn load_file<T>(filename: &Path) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let mut buf = String::new();
    let mut f = match File::open(filename) {
        Ok(f) => Ok(f),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        e => e,
    }
    .context("open file")?;
    f.read_to_string(&mut buf).context("read file")?;
    let data = toml::from_str::<T>(&buf).context("deserialize file contents")?;
    Ok(Some(data))
}

pub fn save_file<T>(filename: &Path, data: T) -> Result<()>
where
    T: Serialize,
{
    let data = toml::to_string(&data).context("serialize data")?;
    fs::write(filename, data).context("write to file")
}

// === Mockable filesystem ===

#[cfg_attr(test, mockall::automock)]
pub trait Filesystem {
    /// Check state of expected symlink on disk
    fn compare_symlink(
        &mut self,
        source: &Path,
        link: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<SymlinkComparison>;

    /// Check state of expected symbolic link on disk
    fn compare_template(
        &mut self,
        target: &Path,
        cache: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<TemplateComparison>;

    /// Removes a file or folder, elevating privileges if needed
    fn remove_file(&mut self, path: &Path) -> Result<()>;

    /// Read contents of file into a string
    fn read_to_string(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<String>;

    /// Write string to file, without elevating privileges
    fn write(&mut self, path: &Path, content: String) -> Result<()>;

    /// Delete parents of target file if they're empty
    fn delete_parents(&mut self, path: &Path, no_ask: bool) -> Result<()>;

    /// Makes a symlink owned by the selected user, elevating privileges as needed
    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// Create directory (and its parents) owned by the selected user,
    /// elevating privileges as needed
    fn create_dir_all(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// Copy readable file to target existing location.
    /// Target file will be owned by the selected user. Privileges elevated as needed.
    fn copy_file(&mut self, source: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()>;

    /// If `owner.is_some`, elevates privileges and sets file to that owner
    /// If `owner.is_none`, ensures file is owned by the current user (elevating privileges if needed)
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
    noconfirm: bool,
}

#[cfg(windows)]
impl RealFilesystem {
    pub fn new(noconfirm: bool) -> RealFilesystem {
        RealFilesystem { noconfirm }
    }
}

#[cfg(windows)]
impl Filesystem for RealFilesystem {
    fn compare_symlink(
        &mut self,
        source: &Path,
        link: &Path,
        _owner: &Option<UnixUser>,
    ) -> Result<SymlinkComparison> {
        let source_state = get_file_state(source).context("get source state")?;
        trace!("Source state: {:#?}", source_state);
        let link_state = get_file_state(link).context("get link state")?;
        trace!("Link state: {:#?}", link_state);

        compare_symlink(source, source_state, link_state)
    }

    fn compare_template(
        &mut self,
        target: &Path,
        cache: &Path,
        _owner: &Option<UnixUser>,
    ) -> Result<TemplateComparison> {
        let target_state = get_file_state(target).context("get state of target")?;
        trace!("Target state: {:#?}", target_state);
        let cache_state = get_file_state(cache).context("get state of cache")?;
        trace!("Cache state: {:#?}", cache_state);

        Ok(compare_template(target_state, cache_state))
    }

    fn remove_file(&mut self, path: &Path) -> Result<()> {
        let metadata = path.symlink_metadata().context("get metadata")?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(path).context("remove directory")
        } else {
            std::fs::remove_file(path).context("remove file")
        }
    }

    fn read_to_string(&mut self, path: &Path, _owner: &Option<UnixUser>) -> Result<String> {
        fs::read_to_string(path).context("read from file")
    }

    fn write(&mut self, path: &Path, content: String) -> Result<()> {
        fs::write(path, content).context("write to file")
    }

    fn delete_parents(&mut self, path: &Path, no_ask: bool) -> Result<()> {
        let mut path = path.parent().context("get parent")?;
        while path.is_dir()
            && path
                .read_dir()
                .context("read the contents of parent directory")?
                .next()
                .is_none()
        {
            if (self.noconfirm || no_ask)
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
        let real_source_path = real_path(target).context("get real path of source file")?;
        if real_source_path.is_dir() {
            fs::symlink_dir(real_source_path, link)
        } else {
            fs::symlink_file(real_source_path, link)
        }
        .context("create symlink")
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
    noconfirm: bool,
    sudo_occurred: bool,
}

#[cfg(unix)]
impl RealFilesystem {
    pub fn new(noconfirm: bool) -> RealFilesystem {
        RealFilesystem {
            sudo_occurred: false,
            noconfirm,
        }
    }

    fn sudo(&mut self, goal: impl AsRef<str>) -> Command {
        sudo_command(&mut self.sudo_occurred, goal)
    }

    fn is_owned_by_user(path: &Path) -> Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let file_uid = path.metadata().context("get file metadata")?.uid();
        let process_uid = unsafe { libc::geteuid() };
        Ok(file_uid == process_uid)
    }

    fn get_file_state(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<FileState> {
        get_file_state_with_owner(&mut self.sudo_occurred, path, owner)
    }

    fn resolve_symlink_destination(
        &mut self,
        link: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<Option<PathBuf>> {
        resolve_symlink_destination_with_owner(&mut self.sudo_occurred, link, owner)
    }
}

#[cfg(unix)]
impl Filesystem for RealFilesystem {
    fn compare_symlink(
        &mut self,
        source: &Path,
        link: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<SymlinkComparison> {
        let source_state = get_file_state(source).context("get source state")?;
        let link_state = self.get_file_state(link, owner).context("get link state")?;

        Ok(match (source_state, link_state) {
            (FileState::Missing, FileState::SymbolicLink(_)) => SymlinkComparison::OnlyTargetExists,
            (_, FileState::SymbolicLink(_)) => {
                let expected_target = real_path(source).context("get real path of source")?;
                match self
                    .resolve_symlink_destination(link, owner)
                    .context("resolve symlink target")?
                {
                    Some(actual_target) if actual_target == expected_target => {
                        SymlinkComparison::Identical
                    }
                    Some(_) | None => SymlinkComparison::Changed,
                }
            }
            (FileState::Missing, FileState::Missing) => SymlinkComparison::BothMissing,
            (_, FileState::Missing) => SymlinkComparison::OnlySourceExists,
            _ => SymlinkComparison::TargetNotSymlink,
        })
    }

    fn compare_template(
        &mut self,
        target: &Path,
        cache: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<TemplateComparison> {
        let target_state = self
            .get_file_state(target, owner)
            .context("get state of target")?;
        let cache_state = get_file_state(cache).context("get state of cache")?;

        Ok(compare_template(target_state, cache_state))
    }

    fn remove_file(&mut self, path: &Path) -> Result<()> {
        let metadata = match path.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let success = self
                    .sudo(format!("removing file {path:?} as root"))
                    .arg("rm")
                    .arg("-r")
                    .arg(path)
                    .spawn()
                    .context("spawn sudo rm command")?
                    .wait()
                    .context("wait for sudo rm command")?
                    .success();

                anyhow::ensure!(success, "sudo rm command failed");
                return Ok(());
            }
            Err(e) => return Err(e).context("get metadata"),
        };
        let result = if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let success = self
                    .sudo(format!("removing file {path:?} as root"))
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

    fn read_to_string(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<String> {
        read_to_string_with_owner(&mut self.sudo_occurred, path, owner)
    }

    fn write(&mut self, path: &Path, content: String) -> Result<()> {
        fs::write(path, content).context("write to file")
    }

    fn delete_parents(&mut self, path: &Path, no_ask: bool) -> Result<()> {
        let mut path = path.parent().context("get parent")?;
        while path.is_dir()
            && path
                .read_dir()
                .context("read the contents of parent directory")?
                .next()
                .is_none()
        {
            if (self.noconfirm || no_ask)
                || ask_boolean(&format!(
                    "Directory at {path:?} is now empty. Delete [y/N]? "
                ))
            {
                match std::fs::remove_dir(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        let success = self
                            .sudo(format!("removing directory {path:?}"))
                            .arg("rmdir")
                            .arg(path)
                            .spawn()
                            .context("spawn sudo rmdir")?
                            .wait()
                            .context("wait for sudo rmdir")?
                            .success();

                        anyhow::ensure!(success, "sudo rmdir failed");
                    }
                    Err(e) => {
                        Err(e).context("remove dir")?;
                    }
                }
            }
            path = path.parent().context(format!("get parent of {path:?}"))?;
        }
        Ok(())
    }

    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        use std::os::unix::fs;

        if let Some(owner) = owner {
            let success = self
                .sudo(format!(
                    "creating symlink {link:?} -> {target:?} from user {owner:?}"
                ))
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
            let success = self
                .sudo(format!(
                    "Creating directory {path:?} from user {owner:?}..."
                ))
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
        use std::io::Write;

        if let Some(owner) = owner {
            let contents = std::fs::read_to_string(source)
                .context("read source file contents as current user")?;
            let mut child = self
                .sudo(format!(
                    "Copying {source:?} -> {target:?} as user {owner:?}"
                ))
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
        if Self::is_owned_by_user(file).context("detect if file is owned by the current user")?
            && owner.is_none()
        {
            // Nothing to do, no need to elevate
            return Ok(());
        }

        let owner = owner.clone().unwrap_or(UnixUser::Name(
            std::env::var("USER").context("get USER env var")?,
        ));

        let success = self
            .sudo(format!("setting owner of {file:?} to user \"{owner}\""))
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
            let success = self
                .sudo(format!(
                    "Copying permissions {source:?} -> {target:?} as user {owner:?}"
                ))
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

// == Dry run Filesystem ==
pub struct DryRunFilesystem {
    file_states: BTreeMap<PathBuf, FileState>,
    #[cfg(unix)]
    sudo_occurred: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum FileState {
    /// None if file is invalid UTF-8
    File(Option<String>),
    SymbolicLink(PathBuf),
    Directory,
    Missing,
}

impl DryRunFilesystem {
    pub fn new() -> DryRunFilesystem {
        DryRunFilesystem {
            file_states: BTreeMap::new(),
            #[cfg(unix)]
            sudo_occurred: false,
        }
    }

    fn get_state(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<FileState> {
        match self.file_states.get(path) {
            Some(state) => Ok(state.clone()),
            None => {
                #[cfg(unix)]
                {
                    get_file_state_with_owner(&mut self.sudo_occurred, path, owner)
                }
                #[cfg(not(unix))]
                {
                    let _ = owner;
                    get_file_state(path)
                }
            }
        }
    }

    #[cfg(unix)]
    fn resolve_symlink_destination(
        &mut self,
        link: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<Option<PathBuf>> {
        resolve_symlink_destination_with_owner(&mut self.sudo_occurred, link, owner)
    }
}

impl Filesystem for DryRunFilesystem {
    fn compare_symlink(
        &mut self,
        source: &Path,
        link: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<SymlinkComparison> {
        let source_state = if let Some(state) = self.file_states.get(source) {
            debug!("Cached (probably not actual) source state: {:?}", state);
            state.clone()
        } else {
            let state = self.get_state(source, &None).context("get source state")?;
            debug!("Source state: {:?}", state);
            state
        };
        let link_state = if let Some(state) = self.file_states.get(link) {
            debug!("Cached (probably not actual) link state: {:?}", state);
            state.clone()
        } else {
            let state = self.get_state(link, owner).context("get link state")?;
            debug!("Link state: {:?}", state);
            state
        };

        Ok(match (source_state, link_state) {
            (FileState::Missing, FileState::SymbolicLink(_)) => SymlinkComparison::OnlyTargetExists,
            (_, FileState::SymbolicLink(target)) => {
                let expected_target = real_path(source).context("get real path of source")?;
                let actual_target = if self.file_states.contains_key(link) {
                    if target == source {
                        Some(expected_target.clone())
                    } else if target == expected_target {
                        Some(target)
                    } else {
                        None
                    }
                } else {
                    #[cfg(unix)]
                    {
                        self.resolve_symlink_destination(link, owner)
                            .context("resolve symlink target")?
                    }
                    #[cfg(not(unix))]
                    {
                        resolve_symlink_destination(link).context("resolve symlink target")?
                    }
                };

                match actual_target {
                    Some(actual_target) if actual_target == expected_target => {
                        SymlinkComparison::Identical
                    }
                    Some(_) | None => SymlinkComparison::Changed,
                }
            }
            (FileState::Missing, FileState::Missing) => SymlinkComparison::BothMissing,
            (_, FileState::Missing) => SymlinkComparison::OnlySourceExists,
            _ => SymlinkComparison::TargetNotSymlink,
        })
    }

    fn compare_template(
        &mut self,
        target: &Path,
        cache: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<TemplateComparison> {
        let target_state = if let Some(state) = self.file_states.get(target) {
            debug!("Cached (probably not actual) target state: {:?}", state);
            state.clone()
        } else {
            let state = self
                .get_state(target, owner)
                .context("get state of target")?;
            debug!("Target state: {:?}", state);
            state
        };
        let cache_state = if let Some(state) = self.file_states.get(cache) {
            debug!("Cached (probably not actual) cache state: {:?}", state);
            state.clone()
        } else {
            let state = self.get_state(cache, &None).context("get state of cache")?;
            debug!("Cache state: {:?}", state);
            state
        };

        Ok(compare_template(target_state, cache_state))
    }

    fn remove_file(&mut self, path: &Path) -> Result<()> {
        debug!("Removing file {:?}", path);
        self.file_states.insert(path.into(), FileState::Missing);
        Ok(())
    }

    fn read_to_string(&mut self, path: &Path, owner: &Option<UnixUser>) -> Result<String> {
        debug!("Reading contents of file {:?}", path);
        match self.get_state(path, owner).context("get file state")? {
            FileState::File(s) => Ok(s.context("invalid utf-8 in template source")?),
            _ => anyhow::bail!("writing to non-file"),
        }
    }

    fn write(&mut self, path: &Path, content: String) -> Result<()> {
        debug!("Writing contents {:?} to file {:?}", content, path);
        self.file_states
            .insert(path.into(), FileState::File(Some(content)));
        Ok(())
    }

    fn delete_parents(&mut self, path: &Path, _no_ask: bool) -> Result<()> {
        debug!(
            "Recursively deleting parents of {:?} if they're empty",
            path
        );
        Ok(())
    }

    fn make_symlink(&mut self, link: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        debug!(
            "Making symlink {:?} -> {:?} (owned by {:?})",
            link, target, owner
        );
        self.file_states
            .insert(link.into(), FileState::SymbolicLink(target.into()));
        Ok(())
    }

    fn create_dir_all(&mut self, mut path: &Path, owner: &Option<UnixUser>) -> Result<()> {
        debug!("Creating directory {:?} (owned by {:?})", path, owner);
        self.file_states.insert(path.into(), FileState::Directory);
        while path.parent().is_some() {
            path = path.parent().unwrap();
            self.file_states.insert(path.into(), FileState::Directory);
        }
        Ok(())
    }

    fn copy_file(&mut self, source: &Path, target: &Path, owner: &Option<UnixUser>) -> Result<()> {
        debug!(
            "Copying file {:?} -> {:?} (target owned by {:?})",
            source, target, owner
        );
        match self
            .get_state(source, &None)
            .context("get state of source file")?
        {
            FileState::File(content) => {
                if self
                    .get_state(target.parent().context("get parent of target")?, &None)
                    .context("get state of target's parent")?
                    == FileState::Directory
                {
                    self.file_states
                        .insert(target.into(), FileState::File(content));
                } else {
                    anyhow::bail!("target's parent is not a directory");
                }
                Ok(())
            }
            s @ (FileState::SymbolicLink(_) | FileState::Directory | FileState::Missing) => {
                anyhow::bail!("file is not regular file but is a {:?}", s);
            }
        }
    }

    fn set_owner(&mut self, file: &Path, owner: &Option<UnixUser>) -> Result<()> {
        debug!("Setting owner of file {:?} to {:?}", file, owner);
        Ok(())
    }

    fn copy_permissions(
        &mut self,
        source: &Path,
        target: &Path,
        owner: &Option<UnixUser>,
    ) -> Result<()> {
        debug!(
            "Copying permissions on files {:?} -> {:?} (target owned by {:?})",
            source, target, owner
        );
        Ok(())
    }
}

// === Comparisons ===

#[cfg(unix)]
fn sudo_command(sudo_occurred: &mut bool, goal: impl AsRef<str>) -> Command {
    if !*sudo_occurred {
        warn!("Elevating permissions ({})", goal.as_ref());
        if !log_enabled!(log::Level::Debug) {
            warn!(
                "To see more than the first time elevated permissions are used, use verbosity 2 or more (-vv)"
            );
        }
        *sudo_occurred = true;
    } else {
        debug!("Elevating permissions ({})", goal.as_ref());
    }
    Command::new("sudo")
}

#[cfg(unix)]
fn get_file_state_with_owner(
    sudo_occurred: &mut bool,
    path: &Path,
    owner: &Option<UnixUser>,
) -> Result<FileState> {
    match try_get_file_state(path) {
        Ok(state) => Ok(state),
        Err(e) if e.kind() == ErrorKind::PermissionDenied && owner.is_some() => {
            get_file_state_as_root(sudo_occurred, path)
        }
        Err(e) => Err(e).context("read contents of file that isn't symbolic or directory"),
    }
}

#[cfg(unix)]
fn get_file_state_as_root(sudo_occurred: &mut bool, path: &Path) -> Result<FileState> {
    let output = sudo_command(sudo_occurred, format!("detecting file state for {path:?} as root"))
        .arg("sh")
        .arg("-c")
        .arg(
            "if [ -L \"$1\" ]; then printf 'symlink\\0'; readlink -- \"$1\"; elif [ -d \"$1\" ]; then printf 'directory\\0'; elif [ -e \"$1\" ]; then printf 'file\\0'; cat -- \"$1\"; else printf 'missing\\0'; fi",
        )
        .arg("sh")
        .arg(path)
        .output()
        .context("run sudo file state probe")?;

    anyhow::ensure!(output.status.success(), "sudo file state probe failed");
    parse_sudo_file_state(output.stdout)
}

#[cfg(not(unix))]
fn resolve_symlink_destination(path: &Path) -> Result<Option<PathBuf>> {
    match real_path(path) {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("get real path of existing symlink"),
    }
}

#[cfg(unix)]
fn resolve_symlink_destination_with_owner(
    sudo_occurred: &mut bool,
    path: &Path,
    owner: &Option<UnixUser>,
) -> Result<Option<PathBuf>> {
    match real_path(path) {
        Ok(path) => Ok(Some(path)),
        Err(e) if e.kind() == ErrorKind::PermissionDenied && owner.is_some() => {
            resolve_symlink_destination_as_root(sudo_occurred, path)
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("get real path of existing symlink"),
    }
}

#[cfg(unix)]
fn resolve_symlink_destination_as_root(
    sudo_occurred: &mut bool,
    path: &Path,
) -> Result<Option<PathBuf>> {
    let output = sudo_command(
        sudo_occurred,
        format!("resolving existing symlink {path:?} as root"),
    )
    .arg("sh")
    .arg("-c")
    .arg(
        "if resolved=$(realpath -- \"$1\" 2>/dev/null); then printf 'resolved\\0%s' \"$resolved\"; elif [ -L \"$1\" ] && [ ! -e \"$1\" ]; then printf 'missing\\0'; else exit 1; fi",
    )
    .arg("sh")
    .arg(path)
    .output()
    .context("run sudo symlink resolution probe")?;

    anyhow::ensure!(
        output.status.success(),
        "sudo symlink resolution probe failed"
    );
    parse_sudo_resolved_path(output.stdout)
}

#[cfg(unix)]
fn read_to_string_with_owner(
    sudo_occurred: &mut bool,
    path: &Path,
    owner: &Option<UnixUser>,
) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == ErrorKind::PermissionDenied && owner.is_some() => {
            read_to_string_as_root(sudo_occurred, path)
        }
        Err(e) => Err(e).context("read from file"),
    }
}

#[cfg(unix)]
fn read_to_string_as_root(sudo_occurred: &mut bool, path: &Path) -> Result<String> {
    let output = sudo_command(sudo_occurred, format!("reading file {path:?} as root"))
        .arg("cat")
        .arg(path)
        .output()
        .context("run sudo cat")?;

    anyhow::ensure!(output.status.success(), "sudo cat failed");
    String::from_utf8(output.stdout)
        .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
        .context("read from file as root")
}

fn try_get_file_state(path: &Path) -> io::Result<FileState> {
    if let Ok(target) = fs::read_link(path) {
        return Ok(FileState::SymbolicLink(target));
    }

    if path.is_dir() {
        return Ok(FileState::Directory);
    }

    match fs::read_to_string(path) {
        Ok(f) => Ok(FileState::File(Some(f))),
        Err(e) if e.kind() == ErrorKind::InvalidData => Ok(FileState::File(None)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(FileState::Missing),
        Err(e) => Err(e),
    }
}

fn get_file_state(path: &Path) -> Result<FileState> {
    try_get_file_state(path).context("read contents of file that isn't symbolic or directory")
}

#[cfg(unix)]
fn parse_sudo_file_state(output: Vec<u8>) -> Result<FileState> {
    let separator = output
        .iter()
        .position(|byte| *byte == b'\0')
        .context("parse sudo file state probe output")?;
    let (kind, payload) = output.split_at(separator);
    let payload = &payload[1..];

    match kind {
        b"symlink" => Ok(FileState::SymbolicLink(PathBuf::from(
            std::ffi::OsString::from_vec(payload.to_vec()),
        ))),
        b"directory" => Ok(FileState::Directory),
        b"file" => match String::from_utf8(payload.to_vec()) {
            Ok(contents) => Ok(FileState::File(Some(contents))),
            Err(_) => Ok(FileState::File(None)),
        },
        b"missing" => Ok(FileState::Missing),
        _ => anyhow::bail!("unexpected sudo file state probe output"),
    }
}

#[cfg(unix)]
fn parse_sudo_resolved_path(output: Vec<u8>) -> Result<Option<PathBuf>> {
    let separator = output
        .iter()
        .position(|byte| *byte == b'\0')
        .context("parse sudo symlink resolution probe output")?;
    let (kind, payload) = output.split_at(separator);
    let payload = &payload[1..];

    match kind {
        b"resolved" => Ok(Some(PathBuf::from(std::ffi::OsString::from_vec(
            payload.to_vec(),
        )))),
        b"missing" => Ok(None),
        _ => anyhow::bail!("unexpected sudo symlink resolution probe output"),
    }
}

#[derive(Debug, PartialEq, Eq)]
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
            TargetNotSymlink => "target already exists and isn't a symlink",
            Changed => "target already exists and doesn't point at source",
            BothMissing => "source and target are missing",
        }
        .fmt(f)
    }
}

#[cfg(windows)]
fn compare_symlink(
    source_path: &Path,
    source_state: FileState,
    link_state: FileState,
) -> Result<SymlinkComparison> {
    Ok(match (source_state, link_state) {
        (FileState::Missing, FileState::SymbolicLink(_)) => SymlinkComparison::OnlyTargetExists,
        (_, FileState::SymbolicLink(t)) => {
            if t == real_path(source_path).context("get real path of source")? {
                SymlinkComparison::Identical
            } else {
                SymlinkComparison::Changed
            }
        }
        (FileState::Missing, FileState::Missing) => SymlinkComparison::BothMissing,
        (_, FileState::Missing) => SymlinkComparison::OnlySourceExists,
        _ => SymlinkComparison::TargetNotSymlink,
    })
}

#[derive(Debug, PartialEq, Eq)]
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
            TargetNotRegularFile => "target already exists and isn't a regular file",
            BothMissing => "cache and target are missing",
        }
        .fmt(f)
    }
}

fn compare_template(target_state: FileState, cache_state: FileState) -> TemplateComparison {
    match (target_state, cache_state) {
        (FileState::File(t), FileState::File(c)) => {
            if t == c {
                TemplateComparison::Identical
            } else {
                TemplateComparison::Changed
            }
        }
        (FileState::File(_), FileState::Missing) => TemplateComparison::OnlyTargetExists,
        (FileState::Missing, FileState::File(_)) => TemplateComparison::OnlyCacheExists,
        (FileState::Missing, FileState::Missing) => TemplateComparison::BothMissing,
        _ => TemplateComparison::TargetNotRegularFile,
    }
}

// === Utility functions ===
pub fn real_path(path: &Path) -> Result<PathBuf, io::Error> {
    let path = std::fs::canonicalize(path)?;
    Ok(platform_dunce(&path))
}

pub fn ask_boolean(prompt: &str) -> bool {
    let mut buf = String::from("a"); // enter the loop at least once
    while !(buf.to_lowercase().starts_with('y')
        || buf.to_lowercase().starts_with('n')
        || buf.is_empty())
    {
        eprintln!("{prompt}");
        buf.clear();
        io::stdin()
            .read_line(&mut buf)
            .expect("Failed to read line from stdin");
    }

    // If empty defaults to no
    buf.to_lowercase().starts_with('y')
}

pub fn is_template(source: &Path) -> Result<bool> {
    if fs::metadata(source)?.is_dir() {
        return Ok(false);
    }

    let mut file = File::open(source).context("open file")?;
    let mut buf = String::new();

    if file.read_to_string(&mut buf).is_err() {
        warn!(
            "File {:?} is not valid UTF-8 - detecting as symlink. Explicitly specify it to silence this message.",
            source
        );
        Ok(false)
    } else {
        Ok(buf.contains("{{"))
    }
}

#[cfg(windows)]
pub fn symlinks_enabled(test_file_path: &Path) -> Result<bool> {
    use std::os::windows::fs;
    debug!(
        "Testing whether symlinks are enabled on path {:?}",
        test_file_path
    );
    let _ = std::fs::remove_file(test_file_path);
    match fs::symlink_file("test.txt", test_file_path) {
        Ok(()) => {
            std::fs::remove_file(test_file_path)
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
    dunce::simplified(path).into()
}

#[cfg(unix)]
pub fn platform_dunce(path: &Path) -> PathBuf {
    path.into()
}

#[cfg(test)]
mod test {
    use super::*;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    fn unique_test_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("dotter-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn simple_remove() {
        let mut fs = DryRunFilesystem::new();
        fs.remove_file(&PathBuf::from("test")).unwrap();
        assert_eq!(
            fs.file_states.get(&PathBuf::from("test")),
            Some(&FileState::Missing)
        );
    }

    #[test]
    fn simple_write_read() {
        let mut fs = DryRunFilesystem::new();
        fs.write(&PathBuf::from("test"), "hello world!".into())
            .unwrap();
        assert_eq!(
            fs.read_to_string(&PathBuf::from("test"), &None).unwrap(),
            "hello world!"
        );
    }

    #[test]
    fn simple_create_dir_all() {
        let mut fs = DryRunFilesystem::new();
        fs.create_dir_all(&PathBuf::from("/home/user/.config"), &None)
            .unwrap();
        assert_eq!(
            fs.get_state(&PathBuf::from("/home"), &None).unwrap(),
            FileState::Directory
        );
        assert_eq!(
            fs.get_state(&PathBuf::from("/home/user"), &None).unwrap(),
            FileState::Directory
        );
        assert_eq!(
            fs.get_state(&PathBuf::from("/home/user/.config"), &None)
                .unwrap(),
            FileState::Directory
        );
    }

    #[test]
    fn full_dry_run() {
        // Emulate creating new template
        let mut fs = DryRunFilesystem::new();
        fs.write(&PathBuf::from("source"), "{{name}}".into())
            .unwrap();
        fs.remove_file(&PathBuf::from("target_dir/target")).unwrap();

        assert_eq!(
            fs.compare_template(
                &PathBuf::from("target_dir/target"),
                &PathBuf::from("cache_dir/cache"),
                &None,
            )
            .unwrap(),
            TemplateComparison::BothMissing
        );

        fs.create_dir_all(&PathBuf::from("target_dir"), &None)
            .unwrap();

        // perform_template_deploy
        assert_eq!(
            fs.read_to_string(&PathBuf::from("source"), &None).unwrap(),
            "{{name}}"
        );
        let rendered = String::from("John");

        // cache
        fs.create_dir_all(&PathBuf::from("cache_dir"), &None)
            .unwrap();
        fs.write(&PathBuf::from("cache_dir/cache"), rendered)
            .unwrap();

        // target
        fs.copy_file(
            &PathBuf::from("cache_dir/cache"),
            &PathBuf::from("target_dir/target"),
            &None,
        )
        .unwrap();
        fs.copy_permissions(
            &PathBuf::from("source"),
            &PathBuf::from("target_dir/target"),
            &None,
        )
        .unwrap();

        // Verify all actions
        assert_eq!(
            fs.file_states.get(&PathBuf::from("source")),
            Some(&FileState::File(Some("{{name}}".into())))
        );
        assert_eq!(
            fs.file_states.get(&PathBuf::from("cache_dir")),
            Some(&FileState::Directory)
        );
        assert_eq!(
            fs.file_states.get(&PathBuf::from("cache_dir/cache")),
            Some(&FileState::File(Some("John".into())))
        );
        assert_eq!(
            fs.file_states.get(&PathBuf::from("target_dir")),
            Some(&FileState::Directory)
        );
        assert_eq!(
            fs.file_states.get(&PathBuf::from("target_dir/target")),
            Some(&FileState::File(Some("John".into())))
        );
    }

    #[test]
    fn dry_run_error_cases() {
        let mut fs = DryRunFilesystem::new();
        fs.write(&PathBuf::from("source"), "hello".into()).unwrap();

        // No parent
        fs.remove_file(&PathBuf::from("some_dir")).unwrap();
        fs.copy_file(
            &PathBuf::from("source"),
            &PathBuf::from("some_dir/target"),
            &None,
        )
        .unwrap_err();

        // Source isn't a file
        fs.make_symlink(&PathBuf::from("link"), &PathBuf::from("target"), &None)
            .unwrap();
        fs.copy_file(&PathBuf::from("link"), &PathBuf::from("link2"), &None)
            .unwrap_err();
    }

    #[cfg(unix)]
    #[test]
    fn compare_symlink_accepts_relative_target_to_same_file() {
        let root = unique_test_path("relative-symlink");
        let source_dir = root.join("source");
        let link_dir = root.join("links");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&link_dir).unwrap();

        let source = source_dir.join("file.txt");
        let link = link_dir.join("file.txt");
        std::fs::write(&source, "hello").unwrap();
        std::os::unix::fs::symlink(PathBuf::from("../source/file.txt"), &link).unwrap();

        let result = RealFilesystem::new(true).compare_symlink(&source, &link, &None);
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(result.unwrap(), SymlinkComparison::Identical);
    }

    #[cfg(unix)]
    #[test]
    fn dry_run_compare_symlink_accepts_relative_target_to_same_file() {
        let root = unique_test_path("dry-run-relative-symlink");
        let source_dir = root.join("source");
        let link_dir = root.join("links");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&link_dir).unwrap();

        let source = source_dir.join("file.txt");
        let link = link_dir.join("file.txt");
        std::fs::write(&source, "hello").unwrap();
        std::os::unix::fs::symlink(PathBuf::from("../source/file.txt"), &link).unwrap();

        let result = DryRunFilesystem::new().compare_symlink(&source, &link, &None);
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(result.unwrap(), SymlinkComparison::Identical);
    }
}
