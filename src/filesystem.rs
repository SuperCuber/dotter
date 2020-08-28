use anyhow::{Context, Result};

use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use toml;

#[derive(Error, Debug)]
pub enum FileLoadError {
    #[error("Failed to open file {filename}")]
    Open {
        filename: PathBuf,
        source: io::Error,
    },

    #[error("Failed to read opened file {filename}")]
    Read {
        filename: PathBuf,
        source: io::Error,
    },

    #[error("Failed to parse file {filename}")]
    Parse {
        filename: PathBuf,
        source: toml::de::Error,
    },
}

pub fn load_file<T>(filename: &Path) -> Result<T, FileLoadError>
where
    T: DeserializeOwned,
{
    let mut buf = String::new();
    let mut f = File::open(filename).map_err(|e| FileLoadError::Open {
        filename: filename.into(),
        source: e,
    })?;
    f.read_to_string(&mut buf)
        .map_err(|e| FileLoadError::Read {
            filename: filename.into(),
            source: e,
        })?;
    toml::from_str::<T>(&buf).map_err(|e| FileLoadError::Parse {
        filename: filename.into(),
        source: e,
    })
}

#[derive(Error, Debug)]
pub enum FileSaveError {
    #[error("Failed to write file {filename}")]
    Write {
        filename: PathBuf,
        source: io::Error,
    },

    #[error("Failed to serialize data")]
    Serialize(#[from] toml::ser::Error),
}

pub fn save_file<T>(filename: &Path, data: T) -> Result<(), FileSaveError>
where
    T: Serialize,
{
    let data = toml::to_string(&data)?;
    fs::write(filename, &data).map_err(|e| FileSaveError::Write {
        filename: filename.into(),
        source: e,
    })?;
    Ok(())
}

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
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use self::SymlinkComparison::*;
        match self {
            Identical => "target points at source",
            OnlySourceExists => "source exists, target missing",
            OnlyTargetExists => "source missing, target exists",
            TargetNotSymlink => "target isn't a symlink",
            Changed => "target isn't point at source",
            BothMissing => "source and target are missing",
        }
        .fmt(f)
    }
}

pub fn compare_symlink(source: &Path, link: &Path) -> Result<SymlinkComparison> {
    let source = match real_path(source) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context(format!("Get canonical path of source {:?}", source))?,
    };

    let link_content = match fs::symlink_metadata(link) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Some(fs::read_link(link).context(format!("Failed to read target of link {:?}", link))?)
        }
        Ok(_) => return Ok(SymlinkComparison::TargetNotSymlink),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context(format!("Failed to read metadata of link {:?}", link))?,
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
    BothMissing,
}

impl std::fmt::Display for TemplateComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use self::TemplateComparison::*;
        match self {
            Identical => "target and cache's contents are equal",
            OnlyCacheExists => "cache exists, target missing",
            OnlyTargetExists => "cache missing, target exists",
            Changed => "target and cache's contents differ",
            BothMissing => "cache and target are missing",
        }
        .fmt(f)
    }
}

pub fn compare_template(target: &Path, cache: &Path) -> Result<TemplateComparison> {
    let target = match fs::read_to_string(target) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context(format!(
            "Failed to read content of target file {:?}",
            target
        ))?,
    };

    let cache = match fs::read_to_string(cache) {
        Ok(c) => Some(c),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Err(e).context(format!("Failed to read contents of cache file {:?}", cache))?,
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

pub fn real_path(path: &Path) -> Result<PathBuf, io::Error> {
    let path = std::fs::canonicalize(&path)?;
    Ok(platform_dunce(path))
}

pub fn ask_boolean(prompt: &str) -> bool {
    let mut buf = String::new();
    while buf.to_lowercase() != "y\n" && buf.to_lowercase() != "n\n" {
        eprintln!("{}", prompt);
        io::stdin()
            .read_line(&mut buf)
            .expect("read line from stdin");
    }

    buf.to_lowercase() == "y\n"
}

pub fn delete_parents(path: &Path, ask: bool) -> Result<()> {
    let mut path = path
        .parent()
        .context(format!("Failed to get parent of {:?}", path))?;
    while path.is_dir()
        && path
            .read_dir()
            .context(format!(
                "Failed to read the contents of directory {:?}",
                path
            ))?
            .next()
            .is_none()
    {
        if !ask
            || ask_boolean(&format!(
                "Directory at {:?} is now empty. Delete [y/n]? ",
                path
            ))
        {
            fs::remove_dir(path).context(format!("Failed to remove directory {:?}", path))?;
        }
        path = path
            .parent()
            .context(format!("Failed to get parent of {:?}", path))?; // I do not expect to reach root from this loop
    }
    Ok(())
}

#[cfg(windows)]
mod filesystem_impl {
    use anyhow::{Context, Result};
    use dunce;

    use std::fs::remove_file;
    use std::os::windows::fs;
    use std::path::{Path, PathBuf};

    pub fn make_symlink(link: &Path, target: &Path) -> Result<()> {
        Ok(fs::symlink_file(
            super::real_path(target).context("Failed to get real path of source file")?,
            link,
        )
        .context("Failed to create symlink")?)
    }

    pub fn symlinks_enabled(test_file_path: &Path) -> Result<bool> {
        debug!(
            "Testing whether symlinks enabled on path {:?}",
            test_file_path
        );
        let _ = remove_file(&test_file_path);
        match fs::symlink_file("test.txt", &test_file_path) {
            Ok(()) => {
                remove_file(&test_file_path)
                    .context(format!("Failed to remove test file {:?}", test_file_path))?;
                Ok(true)
            }
            Err(e) => {
                // os error 1314: A required privilege is not held by the client.
                if e.raw_os_error() == Some(1314) {
                    Ok(true)
                } else {
                    Err(e).context(format!(
                        "Failed to create test symlink at {:?}",
                        test_file_path
                    ))
                }
            }
        }
    }

    pub fn platform_dunce(path: PathBuf) -> PathBuf {
        dunce::simplified(&path).into()
    }
}

#[cfg(unix)]
mod filesystem_impl {
    use anyhow::{Context, Result};

    use std::os::unix::fs;
    use std::path::{Path, PathBuf};

    pub fn make_symlink(link: &Path, target: &Path) -> Result<()> {
        Ok(fs::symlink(
            super::real_path(target).context("Failed to get real path of source file")?,
            link,
        )
        .context("Failed to create symlink")?)
    }

    pub fn symlinks_enabled(_test_file_path: &Path) -> Result<bool> {
        Ok(true)
    }

    pub fn platform_dunce(path: PathBuf) -> PathBuf {
        path
    }
}

#[cfg(not(any(unix, windows)))]
mod filesystem_impl {
    use std::path::Path;
    pub fn make_symlink(link: &Path, target: &Path) {
        panic!("Unsupported platform: neither unix nor windows");
    }

    pub fn symlinks_enabled(_test_file_path: &Path) -> Result<bool> {
        panic!("Unsupported platform: neither unix nor windows");
    }

    pub fn platform_dunce(path: PathBuf) -> PathBuf {
        panic!("Unsupported platform: neither unix nor windows");
    }
}

pub use self::filesystem_impl::*;
