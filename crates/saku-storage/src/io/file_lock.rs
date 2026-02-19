use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::error::IoError;

/// A file-based exclusive lock using fs2.
pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock on the given path. Creates the lock file if needed.
    pub fn acquire(path: &Path) -> Result<Self, IoError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| IoError::LockFailed {
                path: path.to_path_buf(),
                source: e,
            })?;

        file.lock_exclusive().map_err(|e| IoError::LockFailed {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Release the exclusive lock.
    pub fn release(self) -> Result<(), IoError> {
        self.file.unlock().map_err(|e| IoError::UnlockFailed {
            path: self.path,
            source: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join("test.lock");

        let lock = FileLock::acquire(&lock_path).unwrap();
        assert!(lock_path.exists());
        lock.release().unwrap();
    }
}
