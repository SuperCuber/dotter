use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::de::DeserializeOwned;
use toml;

pub fn load_file<T>(filename: &Path) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let mut buf = String::new();
    let mut f = File::open(filename).map_err(|_| "open")?;
    f.read_to_string(&mut buf).map_err(|_| "read")?;
    Ok(toml::from_str::<T>(&buf).map_err(|_| "parse")?)
}
