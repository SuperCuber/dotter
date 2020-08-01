use handlebars::{Handlebars, TemplateRenderError};

use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process;

use args::Options;
use config;
use handlebars_helpers;

pub fn deploy(opt: Options) {
    // Configuration
    info!("Loading configuration...");

    let (files, variables, helpers) =
        config::load_configuration(&opt.local_config, &opt.global_config).unwrap_or_else(|| {
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

    // Prepare handlebars instance
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(|s| s.to_string());
    handlebars_helpers::register_rust_helpers(&mut handlebars);
    handlebars_helpers::register_script_helpers(&mut handlebars, helpers);

    // Deploy files
    for (from, to) in files {
        let to = shellexpand::tilde(&to).into_owned();
        if let Err(msg) = deploy_file(
            &PathBuf::from(&from),
            &PathBuf::from(&to),
            &handlebars,
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
    handlebars: &Handlebars,
    variables: &config::Variables,
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
                handlebars,
                variables,
                cache,
                cache_directory,
                act,
            )?;
        }
        return Ok(());
    }

    if cache {
        let to_cache = &cache_directory.join(from);
        deploy_file(
            from,
            to_cache,
            handlebars,
            variables,
            false,
            cache_directory,
            act,
        )?;
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
            if f_from.read_to_string(&mut content).is_ok() {
                // UTF-8 Compatible file
                let content = substitute_variables(content, handlebars, variables);
                match content {
                    Ok(content) => {
                        let mut f_to = fs::File::create(to)?;
                        f_to.write_all(content.as_bytes())?;
                        f_to.set_permissions(perms)?;
                    }
                    Err(error) => {
                        error!("Error rendering file {:?}: {}", from, error);
                    }
                }
            } else {
                warn!("File {:?} is incompatible with UTF-8, copying byte-for-byte instead of rendering.", from);
                // Binary file or with invalid chars
                f_from.seek(::std::io::SeekFrom::Start(0))?;
                let mut content = Vec::new();
                f_from.read_to_end(&mut content)?;
                let mut f_to = fs::File::create(to)?;
                f_to.write_all(&content)?;
                f_to.set_permissions(perms)?;
            }
        }
    }
    Ok(())
}

fn substitute_variables(
    content: String,
    handlebars: &Handlebars,
    variables: &config::Variables,
) -> Result<String, TemplateRenderError> {
    handlebars.render_template(&content, variables)
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
    use super::config;
    use super::substitute_variables;

    fn table_insert(table: &mut config::Variables, key: &str, value: &str) {
        table.insert(String::from(key), toml::Value::String(String::from(value)));
    }

    fn test_substitute_variables(table: &config::Variables, content: &str, expected: &str) {
        assert_eq!(
            substitute_variables(String::from(content), table).unwrap(),
            expected
        );
    }

    #[test]
    fn test_substitute_variables1() {
        let table = &mut config::Variables::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }}", "bar");
    }

    #[test]
    fn test_substitute_variables2() {
        let table = &mut config::Variables::new();
        table_insert(table, "foo", "bar");
        table_insert(table, "baz", "idk");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar idk");
    }

    #[test]
    fn test_substitute_variables_invalid() {
        let table = &mut config::Variables::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ baz }}", "");
    }

    #[test]
    fn test_substitute_variables_mixed() {
        let table = &mut config::Variables::new();
        table_insert(table, "foo", "bar");
        test_substitute_variables(table, "{{ foo }} {{ baz }}", "bar ");
    }

    #[test]
    fn test_substitute_variables_deep() {
        let table = &mut config::Variables::new();
        let mut person = config::Variables::new();
        person.insert("name".into(), "Jack".into());
        person.insert("family".into(), "Black".into());
        table.insert("person".into(), person.into());
        test_substitute_variables(
            table,
            "Hello, {{person.name}} {{person.family}}!",
            "Hello, Jack Black!",
        );
    }

    #[test]
    fn test_substitute_variables_nonstring() {
        let table = &mut config::Variables::new();
        let mut person = config::Variables::new();
        person.insert("name".into(), "Jonny".into());
        person.insert("age".into(), 5.into());
        table.insert("person".into(), person.into());
        test_substitute_variables(
            table,
            "{{person.name}} can{{#if (lt person.age 18)}}not{{/if}} drink alcohol",
            "Jonny cannot drink alcohol",
        );
    }
}
