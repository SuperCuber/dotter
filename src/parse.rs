use std::path::Path;
use std::fs::File;
use std::io::{Read, Write};

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
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

pub fn save_file<T>(filename: &Path, data: &T) -> Result<(), String>
where
    T: Serialize,
{
    let mut f = File::create(filename).map_err(|_| "open")?;
    let buf = toml::to_string(data).map_err(|_| "serialize")?;
    f.write(buf.as_bytes()).map_err(|_| "write")?;
    Ok(())
}
