use clap;
use parse;

use toml::value::Table;

use std::fs;
use std::io::{Read, Write, Seek};
use std::path::Path;
use std::process;

use filesystem::{parse_path, relativize};

pub fn deploy(
    global: &clap::ArgMatches<'static>,
    specific: &clap::ArgMatches<'static>,
    verbosity: u64,
    act: bool,
) {

    // Configuration
    verb!(verbosity, 1, "Loading configuration...");

    let mut parent = ::std::env::current_dir().expect("Failed to get current directory.");
    let conf = loop {
        if let Ok(conf) = load_configuration(global, verbosity) {
            break Some(conf);
        }
        if let Some(new_parent) = parent.parent().map(|p| p.into()) {
            parent = new_parent;
            verb!(verbosity, 1, "Current directory failed, going one up to {}", parent.to_string_lossy());
        } else {
            verb!(verbosity, 1, "Reached root.");
            break None;
        }
        ::std::env::set_current_dir(&parent).expect("Move a directory up");
    };

    let (files, variables) = conf.unwrap_or_else(|| {
        println!("Failed to find configuration in current or parent directories.");
        process::exit(1);
    });

    // Cache
    let cache = specific.occurrences_of("nocache") == 0;
    verb!(verbosity, 1, "Cache: {}", cache);
    let cache_directory = or_err!(parse_path(specific.value_of("cache_directory").unwrap()));
    if cache {
        verb!(
            verbosity,
            1,
            "Creating cache directory at {:?}",
            cache_directory
        );
        if act && fs::create_dir_all(&cache_directory).is_err() {
            println!("Failed to create cache directory.");
            process::exit(1);
        }
    }

    // Deploy files
    for pair in files {
        let from = or_err!(parse_path(&pair.0));
        let to = or_err!(parse_path(pair.1.as_str().unwrap()));
        if let Err(msg) = deploy_file(
            &from,
            &to,
            &variables,
            verbosity,
            act,
            cache,
            &cache_directory,
        )
        {
            println!("{}", msg);
        }
    }
}

fn deploy_file(
    from: &Path,
    to: &Path,
    variables: &Table,
    verbosity: u64,
    act: bool,
    cache: bool,
    cache_directory: &Path,
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
                verbosity,
                act,
                cache,
                cache_directory,
            )?;
        }
        return Ok(());
    }

    if cache {
        let to_cache = &cache_directory.join(relativize(to));
        deploy_file(
            from,
            to_cache,
            variables,
            verbosity,
            act,
            false,
            cache_directory,
        )?;
        verb!(verbosity, 1, "Copying {:?} to {:?}", to_cache, to);
        if act {
            copy_if_changed(to_cache, to, verbosity)?;
        }
    } else {
        verb!(verbosity, 1, "Templating {:?} to {:?}", from, to);
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

fn load_configuration(
    matches: &clap::ArgMatches<'static>,
    verbosity: u64,
) -> Result<(Table, Table), String> {
    verb!(verbosity, 3, "Deploy args: {:?}", matches);

    // Load files
    let files: Table = parse::load_file(matches.value_of("files").unwrap())?;
    verb!(verbosity, 2, "Files: {:?}", files);

    // Load variables
    let mut variables: Table = parse::load_file(matches.value_of("variables").unwrap())?;
    verb!(verbosity, 2, "Variables: {:?}", variables);

    // Load secrets
    let mut secrets: Table = parse::load_file(matches.value_of("secrets").unwrap())
        .unwrap_or_default();
    verb!(verbosity, 2, "Secrets: {:?}", secrets);

    variables.append(&mut secrets); // Secrets is now empty

    verb!(verbosity, 2, "Variables with secrets: {:?}", variables);

    Ok((files, variables))
}

fn substitute_variables(content: String, variables: &Table) -> String {
    let mut content = content;
    for variable in variables {
        content = content.replace(
            &["{{ ", variable.0, " }}"].concat(),
            variable.1.as_str().unwrap(),
        );
    }
    content.to_string()
}

fn copy_if_changed(from: &Path, to: &Path, verbosity: u64) -> Result<(), ::std::io::Error> {
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
        verb!(
            verbosity,
            2,
            "File {:?} differs from {:?}, copying.",
            from,
            to
        );
        fs::File::create(to)?.write_all(&content_from)?;
    } else {
        verb!(
            verbosity,
            2,
            "File {:?} is the same as {:?}, not copying.",
            from,
            to
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::substitute_variables;
    use super::Table;

    fn table_insert(table: &mut Table, key: &str, value: &str) {
        table.insert(
            String::from(key),
            ::toml::Value::String(String::from(value)),
        );
    }

    fn test_substitute_variables(table: &Table, content: &str, expected: &str) {
        assert_eq!(substitute_variables(String::from(content), table), expected);
    }

    #[test]
    fn test_substitute_variables1() {
        let table = &mut Table::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }}", "bar");
    }

    #[test]
    fn test_substitute_variables2() {
        let table = &mut Table::new();
        table_insert(table, "foo", "bar");
        table_insert(table, "baz", "idk");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar idk");
    }

    #[test]
    fn test_substitute_variables_invalid() {
        let table = &mut Table::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ baz }}", "{{ baz }}");
    }

    #[test]
    fn test_substitute_variables_mixed() {
        let table = &mut Table::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar {{ baz }}");
    }

}
