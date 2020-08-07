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

pub fn symlink_equals(link: &Path, target: &Path) -> bool {
    match fs::symlink_metadata(link) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                fs::read_link(link).expect("read symlink contents") == target
            } else {
                false
            }
        }
        Err(e) => {
            error!(
                "Couldn't check whether {:?} is a symlink because {}",
                link, e
            );
            process::exit(1);
        }
    }
}

pub fn template_equals(target: &Path, cache: &Path) -> bool {
    fs::read_to_string(target).unwrap_or_else(|e| {
        error!("Failed to read file {:?} because {}", target, e);
        String::new()
    }) == fs::read_to_string(cache).expect("read template in cache")
}

pub fn real_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(shellexpand::tilde(&path.to_string_lossy()).to_string()).unwrap_or_else(
        |e| {
            error!("Failed to canonicalize {:?}: {}", path, e);
            process::exit(1);
        },
    )
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
    use std::os::windows::fs;
    use std::path::Path;
    pub fn make_symlink(link: &Path, target: &Path) {
        if let Err(e) = fs::symlink_file(target, link) {
            error!("Failed to create symlink at {:?} because {}", target, e);
        }
    }
}

#[cfg(unix)]
mod filesystem_impl {
    use std::os::unix::fs;
    use std::path::Path;
    pub fn make_symlink(link: &Path, target: &Path) {
        if let Err(e) = fs::symlink(target, link) {
            error!("Failed to create symlink at {:?} because {}", target, e);
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod filesystem_impl {
    use std::path::Path;
    pub fn make_symlink(link: &Path, target: &Path) {
        panic!("Unsupported platform: neither unix nor windows");
    }
}

pub use self::filesystem_impl::*;
