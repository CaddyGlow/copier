use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{CopierError, Result};

pub fn read(path: &Utf8Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(Into::into)
}

pub fn read_to_string(path: &Utf8Path) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|e| CopierError::Io(io::Error::new(e.kind(), format!("{}: {}", path, e))))
}

pub fn write(path: &Utf8Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data).map_err(Into::into)
}

pub fn write_string(path: &Utf8Path, contents: &str) -> Result<()> {
    write(path, contents.as_bytes())
}

pub fn append(path: &Utf8Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn create_dir_all(path: &Utf8Path) -> Result<()> {
    fs::create_dir_all(path).map_err(Into::into)
}

pub fn ensure_parent(path: &Utf8Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    Ok(())
}

pub fn canonicalize(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = fs::canonicalize(path)?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|p| CopierError::InvalidUtfPath(p.to_string_lossy().into_owned()))
}

pub fn metadata(path: &Utf8Path) -> Result<fs::Metadata> {
    fs::metadata(path).map_err(Into::into)
}

pub fn file_exists(path: &Utf8Path) -> bool {
    path.exists()
}

pub fn open_read(path: &Utf8Path) -> Result<File> {
    File::open(path).map_err(Into::into)
}

pub fn open_write(path: &Utf8Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(path).map_err(Into::into)
}

pub fn copy_permissions(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    let metadata = fs::metadata(from)?;
    fs::set_permissions(to, metadata.permissions()).map_err(Into::into)
}

pub fn read_permissions(path: &Utf8Path) -> Result<fs::Permissions> {
    let metadata = fs::metadata(path)?;
    Ok(metadata.permissions())
}

pub fn set_permissions(path: &Utf8Path, perms: fs::Permissions) -> Result<()> {
    fs::set_permissions(path, perms).map_err(Into::into)
}

pub fn remove_file(path: &Utf8Path) -> Result<()> {
    fs::remove_file(path).map_err(Into::into)
}

pub fn remove_dir_all(path: &Utf8Path) -> Result<()> {
    fs::remove_dir_all(path).map_err(Into::into)
}

pub fn read_to_end(file: &mut File) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn write_all(file: &mut File, data: &[u8]) -> Result<()> {
    file.write_all(data).map_err(Into::into)
}

pub fn flush(file: &mut File) -> Result<()> {
    file.flush().map_err(Into::into)
}

pub fn rename(from: &Utf8Path, to: &Utf8Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(from, to).map_err(Into::into)
}

pub fn read_dir(path: &Utf8Path) -> io::Result<fs::ReadDir> {
    fs::read_dir(path)
}
