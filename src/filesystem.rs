use std::path::{Path, PathBuf};
use std::process;

pub fn parse_path(path: &str) -> Result<PathBuf, String> {
    let mut command = format!("realpath -ms --relative-to=. {}", path);
    if let Ok(working_dir) = ::std::env::current_dir() {
        command = format!("cd {:?} && {}", working_dir, command);
    }

    let output = process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output();
    if output.is_err() {
        return Err(["Couldn't get output of command ", &command].concat());
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

pub fn ignore_absolute_join(one: &Path, two: &Path) -> PathBuf {
    relativize(one).join(relativize(two))
}

fn relativize(path: &Path) -> PathBuf {
    if path.is_relative() { path.into() }
    else {
        let mut answer = PathBuf::new();
        let mut components = path.components();
        components.next();
        for comp in components {
            answer = answer.join(comp.as_os_str());
        }
        answer
    }
}
