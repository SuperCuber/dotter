use std::path::{Path, PathBuf};
use std::process;

pub fn parse_path(path: &str) -> Result<PathBuf, String> {
    let command = format!("realpath -ms {}", path);

    let output = process::Command::new("sh").arg("-c").arg(&command).output();
    if output.is_err() {
        return Err(format!("Couldn't get output of command {}", &command));
    }
    let output = output.unwrap();

    if !output.stderr.is_empty() {
        let msg = format!("Unable to resolve path using '{}':\n{}",
                          command,
                          String::from_utf8_lossy(&output.stderr).trim());
        return Err(msg);
    }

    let resolved_out = String::from_utf8_lossy(&output.stdout);
    Ok(resolved_out.trim().into())
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
