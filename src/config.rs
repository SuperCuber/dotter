use toml::value::Table;

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
