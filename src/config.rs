use parse;

use args::{Target, TargetType, Action, ActionEnum, GlobalOptions};

use toml::value::Table;
use std::process;

pub fn config(
    target: Target,
    action: Action,
    opt: GlobalOptions,
) {
    let verbosity = opt.verbose;

    let filename = match target.as_type() {
        TargetType::File => opt.files,
        TargetType::Variable => opt.variables,
        TargetType::Secret => opt.secrets,
    };
    verb!(verbosity, 1, "Operating on file {:?}", filename);

    let mut parsed: Table = or_err!(parse::load_file(&filename));
    verb!(verbosity, 2, "Loaded data: {:?}", parsed);

    match action.as_enum() {
        ActionEnum::Add { from: key, to: value } => {
            let value = ::toml::Value::String(String::from(value));
            verb!(
                verbosity,
                1,
                "Inserting {} -> {:?}.\nBefore: {}",
                key,
                value,
                pretty_print(&parsed)
            );
            if opt.act {
                parsed.insert(key, value);
            }
            verb!(verbosity, 1, "After: {}", pretty_print(&parsed));
        }
        ActionEnum::Remove(key) => {
            verb!(
                verbosity,
                1,
                "Removing {}.\nBefore: {}",
                key,
                pretty_print(&parsed)
            );
            if opt.act {
                parsed.remove(&key);
            }
            verb!(verbosity, 1, "After: {:?}", pretty_print(&parsed));
        }
        ActionEnum::Display => {
            println!("{}", pretty_print(&parsed));
        }
    }

    or_err!(parse::save_file(&filename, &parsed));
}

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
