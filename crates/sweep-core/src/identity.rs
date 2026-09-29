use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub modified_seconds: i64,
    pub modified_nanoseconds: i64,
}

impl FileIdentity {
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        Self::from_metadata(&metadata)
    }

    #[cfg(unix)]
    pub fn from_metadata(metadata: &fs::Metadata) -> io::Result<Self> {
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
        })
    }

    #[cfg(not(unix))]
    pub fn from_metadata(_metadata: &fs::Metadata) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Sweep requires Unix filesystem identity semantics",
        ))
    }

    pub fn still_matches(&self, path: &Path) -> io::Result<bool> {
        Ok(Self::from_path(path)? == *self)
    }
}
