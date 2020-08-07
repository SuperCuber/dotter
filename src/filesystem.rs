use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use shellexpand;

use toml;

pub fn load_file<T>(filename: &Path) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let mut buf = String::new();
    let mut f = File::open(filename).map_err(|_| "open")?;
    f.read_to_string(&mut buf).map_err(|_| "read")?;
    Ok(toml::from_str::<T>(&buf).map_err(|_| "parse")?)
}

pub fn save_file<T>(filename: &Path, data: T) -> Result<(), String>
where
    T: Serialize,
{
    let data = toml::to_string(&data).map_err(|_| "serialize")?;
    fs::write(filename, &data).map_err(|_| "write")?;
    Ok(())
}

pub enum FileCompareState {
    Equal,
    Missing,
    Changed,
}

pub fn compare_symlink(link: &Path, target: &Path) -> FileCompareState {
    match fs::symlink_metadata(link) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                if fs::read_link(link).expect("read symlink contents") == target {
                    FileCompareState::Equal
                } else {
                    FileCompareState::Changed
                }
            } else {
                FileCompareState::Changed
            }
        }
        Err(e) => {
            if e.raw_os_error() == Some(2) {
                return FileCompareState::Missing;
            }
            error!(
                "Couldn't check whether {:?} is a symlink because {}",
                link, e
            );
            process::exit(1);
        }
    }
}

pub fn compare_template(target: &Path, cache: &Path) -> FileCompareState {
    match fs::read_to_string(target) {
        Ok(content) => if content == fs::read_to_string(cache).expect("read template in cache") {
            FileCompareState::Equal
        } else { FileCompareState::Changed }
        Err(e) => {
            if e.raw_os_error() == Some(2) {
                return FileCompareState::Missing;
            }
            error!("Failed to read file {:?} because {}", target, e);
            process::exit(1);
        }
    }
}

pub fn real_path(path: &Path) -> PathBuf {
    let path = PathBuf::from(path);
    let path = shellexpand::tilde(&path.to_string_lossy()).to_string();
    let path = std::fs::canonicalize(&path).unwrap_or_else(|e| {
            error!("Failed to canonicalize {:?}: {}", path, e);
            process::exit(1);
        },
    );
    platform_dunce(path)
}

pub fn ask_boolean(prompt: &str) -> bool {
    let mut buf = String::new();
    while buf.to_lowercase() != "y" && buf.to_lowercase() != "n" {
        eprintln!("{}", prompt);
        io::stdin()
            .read_line(&mut buf)
            .expect("read line from stdin");
    }

    buf.to_lowercase() == "y"
}

pub fn delete_parents(path: &Path, ask: bool) {
    let mut path = path.parent().expect("path has parent");
    while path.is_dir()
        && path
            .read_dir()
            .expect("read directory")
            .collect::<Vec<_>>()
            .is_empty()
    {
        if !ask
            || ask_boolean(&format!(
                "Directory at {:?} is now empty. Delete [y/n]? ",
                path
            ))
        {
            fs::remove_dir(path).expect("delete directory");
        }
        path = path.parent().expect("path has parent"); // I do not expect to reach root from this loop
    }
}

#[cfg(windows)]
mod filesystem_impl {
    use dunce;

    use std::process;
    use std::os::windows::fs;
    use std::fs::remove_file;
    use std::path::{Path, PathBuf};

    pub fn make_symlink(link: &Path, target: &Path) {
        if let Err(e) = fs::symlink_file(target, link) {
            error!("Failed to create symlink at {:?} because {}", target, e);
        }
    }

    pub fn symlinks_enabled(test_file_path: &Path) -> bool {
        debug!("Testing whether symlinks enabled on path {:?}", test_file_path);
        let _ = remove_file(&test_file_path);
        match fs::symlink_file("test.txt", &test_file_path) {
            Ok(()) => {
                remove_file(&test_file_path).expect("remove test file");
                true
            },
            Err(e) => {
                // os error 1314: A required privilege is not held by the client.
                if e.raw_os_error() != Some(1314) {
                    error!("Failed to create test symlink at path {:?} because {}", test_file_path, e);
                    process::exit(1);
                } else { false }
            }
        }
    }

    pub fn platform_dunce(path: PathBuf) -> PathBuf {
        dunce::simplified(&path).into()
    }
}

#[cfg(unix)]
mod filesystem_impl {
    use std::os::unix::fs;
    use std::path::{Path, PathBuf};
    pub fn make_symlink(link: &Path, target: &Path) {
        if let Err(e) = fs::symlink(target, link) {
            error!("Failed to create symlink at {:?} because {}", target, e);
        }
    }

    pub fn symlinks_enabled() -> bool {
        true
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

    pub fn symlinks_enabled() -> bool {
        panic!("Unsupported platform: neither unix nor windows");
    }

    pub fn platform_dunce(path: PathBuf) -> PathBuf {
        panic!("Unsupported platform: neither unix nor windows");
    }
}

pub use self::filesystem_impl::*;
