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
    let mut f = File::open(filename).or_else(|_| { Err("open") })?;
    f.read_to_string(&mut buf).or_else(|_| { Err("read") })?;
    Ok(toml::from_str::<T>(&buf).or_else(|_| { Err("parse") })?)
}

pub fn save_file<T>(filename: &Path, data: &T) -> Result<(), String>
where
    T: Serialize,
{
    let mut f = File::create(filename).or_else(|_| { Err("open") })?;
    let buf = toml::to_string(data).or_else(|_| { Err("serialize") })?;
    f.write(buf.as_bytes()).or_else(|_| { Err("write") })?;
    Ok(())
}
