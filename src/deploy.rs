use config;
use args::Options;

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::process;

use filesystem::{canonicalize, relativize};

pub fn deploy(opt: Options) {
    // Configuration
    info!("Loading configuration...");

    let mut parent = ::std::env::current_dir().expect("Failed to get current directory.");
    let conf = loop {
        match config::load_configuration(&opt.local_config, &opt.global_config) {
            Ok(conf) => break Some(conf),
            Err(e) => {
                if let Some(new_parent) = parent.parent().map(|p| p.into()) {
                    parent = new_parent;
                    warn!(
                        "Current directory failed on step: {}, going one up to {:?}",
                        e, parent
                    );
                } else {
                    warn!("Reached root.");
                    break None;
                }
                ::std::env::set_current_dir(&parent).expect("Move a directory up");
            }
        }
    };

    let (files, variables) = conf.unwrap_or_else(|| {
        error!("Failed to find configuration in current or parent directories.");
        process::exit(1);
    });

    // Cache
    debug!("Cache: {}", opt.cache);
    if opt.cache {
        info!("Creating cache directory at {:?}", &opt.cache_directory);
        if opt.act && fs::create_dir_all(&opt.cache_directory).is_err() {
            error!("Failed to create cache directory.");
            process::exit(1);
        }
    }

    // Deploy files
    for pair in files {
        let from = canonicalize(&pair.0).unwrap_or_else(|err| {
            error!("Failed to canonicalize path {:?}: {}", pair.0, err);
            process::exit(1);
        });
        let to = canonicalize(&pair.1).unwrap_or_else(|err| {
            error!("Failed to canonicalize path {:?}: {}", pair.1, err);
            process::exit(1);
        });
        if let Err(msg) = deploy_file(
            &from,
            &to,
            &variables,
            opt.cache,
            &opt.cache_directory,
            opt.act,
        ) {
            warn!("Failed to deploy {:?} -> {:?}: {}", &from, &to, msg);
        }
    }
}

fn deploy_file(
    from: &Path,
    to: &Path,
    variables: &BTreeMap<String, String>,
    cache: bool,
    cache_directory: &Path,
    act: bool,
) -> Result<(), ::std::io::Error> {
    // Create target directory
    if act {
        let to_parent = to.parent().unwrap_or(to);
        fs::create_dir_all(to_parent)?;
    }

    // If directory, recurse in
    let meta_from = fs::metadata(from)?;
    if meta_from.file_type().is_dir() {
        for entry in fs::read_dir(from)? {
            let entry = entry?.file_name();
            deploy_file(
                &from.join(&entry),
                &to.join(&entry),
                variables,
                cache,
                cache_directory,
                act,
            )?;
        }
        return Ok(());
    }

    if cache {
        let to_cache = &cache_directory.join(relativize(to));
        deploy_file(from, to_cache, variables, false, cache_directory, act)?;
        info!("Copying {:?} to {:?}", to_cache, to);
        if act {
            copy_if_changed(to_cache, to)?;
        }
    } else {
        info!("Templating {:?} to {:?}", from, to);
        let perms = meta_from.permissions();
        if act {
            let mut f_from = fs::File::open(from)?;
            let mut content = String::new();
            let mut f_to = fs::File::create(to)?;
            if f_from.read_to_string(&mut content).is_ok() {
                // UTF-8 Compatible file
                let content = substitute_variables(content, variables);
                f_to.write_all(content.as_bytes())?;
            } else {
                // Binary file or with invalid chars
                f_from.seek(::std::io::SeekFrom::Start(0))?;
                let mut content = Vec::new();
                f_from.read_to_end(&mut content)?;
                f_to.write_all(&content)?;
            }
            f_to.set_permissions(perms)?;
        }
    }
    Ok(())
}

fn substitute_variables(content: String, variables: &BTreeMap<String, String>) -> String {
    let mut content = content;
    for variable in variables {
        content = content.replace(&["{{ ", variable.0, " }}"].concat(), variable.1);
    }
    content
}

fn copy_if_changed(from: &Path, to: &Path) -> Result<(), ::std::io::Error> {
    let mut content_from = Vec::new();
    let mut content_to = Vec::new();

    let mut copy = false;

    fs::File::open(from)?.read_to_end(&mut content_from)?;
    if let Ok(mut f_to) = fs::File::open(to) {
        f_to.read_to_end(&mut content_to)?;
    } else {
        copy = true;
    }

    let copy = copy || content_from != content_to;

    if copy {
        info!("File {:?} differs from {:?}, copying.", from, to);
        fs::File::create(to)?.write_all(&content_from)?;
    } else {
        info!("File {:?} is the same as {:?}, not copying.", from, to);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::substitute_variables;
    use super::BTreeMap;

    fn table_insert(table: &mut BTreeMap<String, String>, key: &str, value: &str) {
        table.insert(
            String::from(key),
            String::from(value),
        );
    }

    fn test_substitute_variables(table: &BTreeMap<String, String>, content: &str, expected: &str) {
        assert_eq!(substitute_variables(String::from(content), table), expected);
    }

    #[test]
    fn test_substitute_variables1() {
        let table = &mut BTreeMap::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }}", "bar");
    }

    #[test]
    fn test_substitute_variables2() {
        let table = &mut BTreeMap::new();
        table_insert(table, "foo", "bar");
        table_insert(table, "baz", "idk");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar idk");
    }

    #[test]
    fn test_substitute_variables_invalid() {
        let table = &mut BTreeMap::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ baz }}", "{{ baz }}");
    }

    #[test]
    fn test_substitute_variables_mixed() {
        let table = &mut BTreeMap::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar {{ baz }}");
    }
}
