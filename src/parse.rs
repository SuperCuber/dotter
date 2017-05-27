use std::fs::File;
use std::io::Read;

use serde::de::DeserializeOwned;
use toml;

pub fn load_file<T>(filename: &str) -> Result<T, String>
    where T: DeserializeOwned
{
    let mut buf = String::new();
    let mut f = File::open(filename)
        .or_else(|_| Err(["Error: file ", filename, " not found."].concat()))?;
    f.read_to_string(&mut buf)
        .or_else(|_| Err(["Error: Couldn't read ", filename].concat()))?;
    Ok(toml::from_str::<T>(&buf)
       .or_else(|_| Err(["Error: Couldn't parse ", filename].concat()))?)
}

#[derive(Debug,Serialize, Deserialize)]
pub struct Config {
    pub files: Option<toml::value::Table>,
    pub variables: Option<toml::value::Table>,
}

#[derive(Debug,Serialize, Deserialize)]
pub struct Secrets {
    pub secrets: Option<toml::value::Table>,
}
