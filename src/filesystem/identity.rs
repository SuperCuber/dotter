use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

/// Identity of the directory entry itself, without following a final symlink.
/// This catches path aliases without assuming case sensitivity or Unicode rules.
pub fn file_identity(path: &Path) -> Result<Option<(u64, u64)>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("inspect file identity"),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Some((metadata.dev(), metadata.ino())))
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        windows_identity(path).map(Some)
    }
}

#[cfg(windows)]
fn windows_identity(path: &Path) -> Result<(u64, u64)> {
    use std::ffi::c_void;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    // BY_HANDLE_FILE_INFORMATION from the Windows API. The stable standard
    // library does not yet expose volume serial numbers and file indices.
    #[repr(C)]
    #[derive(Default)]
    struct FileInformation {
        _attributes: u32,
        _creation_time: [u32; 2],
        _last_access_time: [u32; 2],
        _last_write_time: [u32; 2],
        volume_serial_number: u32,
        _file_size_high: u32,
        _file_size_low: u32,
        _number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn get_file_information_by_handle(handle: *mut c_void, info: *mut FileInformation) -> i32;
    }

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = fs::OpenOptions::new()
        .access_mode(0) // Metadata access only, without reading file contents.
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .context("open file identity handle")?;
    let mut information = FileInformation::default();
    // SAFETY: the handle is alive, and the writable buffer has the API's exact
    // C layout. OPEN_REPARSE_POINT identifies a link rather than its referent.
    if unsafe { get_file_information_by_handle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(io::Error::last_os_error()).context("read file identity");
    }
    Ok((
        information.volume_serial_number.into(),
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low),
    ))
}
