use shellexpand;

use std::path::{Path, PathBuf};
use std::{io, fs};

pub fn canonicalize<P: AsRef<Path>>(path: P) -> Result<PathBuf, io::Error> {
    fs::canonicalize(shellexpand::tilde(&path.as_ref().to_string_lossy()).into_owned())
}

pub fn relativize(path: &Path) -> PathBuf {
    if path.is_relative() {
        path.into()
    } else {
        let mut answer = PathBuf::new();
        let mut components = path.components();
        components.next();
        for comp in components {
            answer = answer.join(comp.as_os_str());
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::relativize;

    fn test_relativize(arg: &str, expected: &str) {
        let arg: super::PathBuf = arg.into();
        assert_eq!(relativize(&arg).as_os_str(), expected);
    }

    #[test]
    fn test_relativize_relative_single() {
        test_relativize("foo", "foo");
    }

    #[test]
    fn test_relativize_relative_multiple() {
        test_relativize("foo/bar/baz", "foo/bar/baz");
    }

    #[test]
    fn test_relativize_absolute_single() {
        test_relativize("/foo", "foo");
    }

    #[test]
    fn test_relativize_absolute_multiple() {
        test_relativize("/foo/bar/baz", "foo/bar/baz");
    }

}
