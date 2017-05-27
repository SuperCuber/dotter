use std::io::Read;
use std::fs::File;

use toml;
use serde::de::DeserializeOwned;

pub fn load_file<T>(filename: &str) -> Result<T, String>
    where T: DeserializeOwned
{
    let mut buf = String::new();
    let mut f = File::open(filename)
        .or_else(|_| Err(["Error: file ", filename, " not found."].concat()))?;
    f.read_to_string(&mut buf)
        .or_else(|_| Err(String::from("ErrorMessage")))?;
    toml::from_str(&buf)
        .or_else(|_| Err(String::from("ErrorMessage")))?
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub files: Option<toml::value::Table>,
    pub variables: Option<toml::value::Table>,
}

impl ::std::fmt::Display for Config {
    fn fmt(&self, formatter: &mut ::std::fmt::Formatter)
        -> ::std::result::Result<(), ::std::fmt::Error> {
        formatter.write_str(&format!("Config [ files: {:?}, variables: {:?} ]",
                                    self.files, self.variables))
    }
}
