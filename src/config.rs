use parse;

use args::{Target, TargetType, Action, ActionEnum, GlobalOptions};

use toml::value::Table;
use std::process;

pub fn config(
    target: Target,
    action: Action,
    opt: GlobalOptions,
) {
    let filename = match target.as_type() {
        TargetType::File => opt.files,
        TargetType::Variable => opt.variables,
        TargetType::Secret => opt.secrets,
    };
    debug!("Target: {:?}", filename);

    let mut parsed: Table = parse::load_file(&filename).unwrap_or_else(|err_step| {
        error!("Failed to load file {:?} on step {}", filename, err_step);
        process::exit(1);
    });
    info!("Loaded data: {:?}", parsed);

    match action.as_enum() {
        ActionEnum::Add { from: key, to: value } => {
            let value = ::toml::Value::String(value);
            info!("Inserting {} -> {:?}.", key, value);
            debug!("Before: {}", pretty_print(&parsed));
            if opt.act {
                parsed.insert(key, value);
            }
            debug!("After: {}", pretty_print(&parsed));
        }
        ActionEnum::Remove(key) => {
            info!("Removing {}.", key);
            debug!("Before: {}", pretty_print(&parsed));
            if opt.act {
                parsed.remove(&key);
            }
            debug!("After: {:?}", pretty_print(&parsed));
        }
        ActionEnum::Display => {
            println!("{}", pretty_print(&parsed));
        }
    }

    if let Err(err_step) = parse::save_file(&filename, &parsed) {
        error!("Failed to save file {:?} on step {}", filename, err_step);
        process::exit(1);
    }
}

/// Panics if table's values aren't strings
fn pretty_print(table: &Table) -> String {
    let mut output = String::new();
    for pair in table {
        output.push_str(pair.0);
        output.push_str(" = ");
        output.push_str(pair.1.as_str().unwrap());
        output.push('\n');
    }
    output.pop(); // Last \n
    output
}
