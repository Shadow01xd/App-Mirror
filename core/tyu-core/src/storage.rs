//! Capability-relative filesystem operations (including symlink resolution).
use crate::{Result, StorageOp, StorageReply, TyuErrorCode};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use tyu_types::StorageMetadata;

pub trait StorageProvider: Send + Sync {
    fn list(&self, path: &str) -> Result<Vec<StorageMetadata>>;
    fn stat(&self, path: &str) -> Result<StorageMetadata>;
    fn read_range(&self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>>;
    fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<()>;
    fn mkdir(&self, path: &str) -> Result<()>;
    fn rename(&self, from: &str, to: &str) -> Result<()>;
    fn delete(&self, path: &str) -> Result<()>;
    fn execute(&self, op: StorageOp) -> Result<StorageReply> {
        Ok(match op {
            StorageOp::List { path } => StorageReply::Entries(self.list(&path)?),
            StorageOp::Stat { path } => StorageReply::Entries(vec![self.stat(&path)?]),
            StorageOp::Read {
                path,
                offset,
                length,
            } => StorageReply::Data(self.read_range(&path, offset, length)?),
            StorageOp::Write { path, offset, data } => {
                self.write(&path, offset, &data)?;
                StorageReply::Done
            }
            StorageOp::CreateDir { path } => {
                self.mkdir(&path)?;
                StorageReply::Done
            }
            StorageOp::Rename { from, to } => {
                self.rename(&from, &to)?;
                StorageReply::Done
            }
            StorageOp::Delete { path } => {
                self.delete(&path)?;
                StorageReply::Done
            }
        })
    }
}
pub struct LocalFilesystemProvider {
    root: Dir,
    max_file_size: u64,
}
pub fn relative(path: &str, allow_root: bool) -> Result<&str> {
    if (path.is_empty() || path == ".") && allow_root {
        return Ok(".");
    }
    if path.is_empty() || path.len() > 1024 || path.contains('\\') || path.starts_with('/') {
        return Err(TyuErrorCode::PathDenied.into());
    }
    for component in path.split('/') {
        tyu_protocol::validate_filename(component)?;
    }
    Ok(path)
}
impl LocalFilesystemProvider {
    pub fn new(root: &Path, max_file_size: u64) -> Result<Self> {
        Ok(Self {
            root: Dir::open_ambient_dir(root, cap_std::ambient_authority())?,
            max_file_size,
        })
    }
}
impl StorageProvider for LocalFilesystemProvider {
    fn list(&self, path: &str) -> Result<Vec<StorageMetadata>> {
        let path = relative(path, true)?;
        let mut result = Vec::new();
        for entry in self.root.read_dir(path)? {
            let entry = entry?;
            if result.len() >= 256 {
                return Err(TyuErrorCode::Limit.into());
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| TyuErrorCode::PathDenied)?;
            tyu_protocol::validate_filename(&name)?;
            let child = if path == "." {
                name
            } else {
                format!("{path}/{name}")
            };
            // Metadata through root ensures a directory entry cannot follow an escaping link.
            result.push(self.stat(&child)?);
        }
        result.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(result)
    }
    fn stat(&self, path: &str) -> Result<StorageMetadata> {
        let path = relative(path, true)?;
        let m = self.root.metadata(path)?;
        Ok(StorageMetadata {
            path: path.into(),
            is_dir: m.is_dir(),
            size: m.len(),
        })
    }
    fn read_range(&self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>> {
        if length as usize > tyu_protocol::MAX_CHUNK {
            return Err(TyuErrorCode::Limit.into());
        }
        let mut file = self.root.open(relative(path, false)?)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut result = Vec::with_capacity(length as usize);
        file.take(length as u64).read_to_end(&mut result)?;
        Ok(result)
    }
    fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        if data.len() > tyu_protocol::MAX_CHUNK
            || offset
                .checked_add(data.len() as u64)
                .is_none_or(|n| n > self.max_file_size)
        {
            return Err(TyuErrorCode::Limit.into());
        }
        let mut file = self.root.open_with(
            relative(path, false)?,
            OpenOptions::new().write(true).create(true),
        )?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_data()?;
        Ok(())
    }
    fn mkdir(&self, path: &str) -> Result<()> {
        self.root.create_dir(relative(path, false)?)?;
        Ok(())
    }
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.root
            .rename(relative(from, false)?, &self.root, relative(to, false)?)?;
        Ok(())
    }
    fn delete(&self, path: &str) -> Result<()> {
        let path = relative(path, false)?;
        if self.root.symlink_metadata(path)?.is_dir() {
            self.root.remove_dir(path)?;
        } else {
            self.root.remove_file(path)?;
        }
        Ok(())
    }
}
