use std::fs;
use std::path::{Path, PathBuf};

use crate::error::IoError;

/// Returns the backup directory for a given store file path.
/// The backup directory is a `backups/` sibling directory next to the store file.
pub fn get_backup_dir(store_path: &Path) -> PathBuf {
    let parent = store_path.parent().unwrap_or(Path::new("."));
    parent.join("backups")
}

/// Returns a unique backup file path based on the current timestamp.
pub fn get_backup_path(store_path: &Path) -> PathBuf {
    let backup_dir = get_backup_dir(store_path);
    let timestamp = jiff::Timestamp::now().to_string();
    let filename = format!("{:?}-{}", store_path.file_name(), timestamp);
    backup_dir.join(filename)
}

/// Create a backup of the store file. Returns the number of bytes copied,
/// or 0 if the source file doesn't exist.
pub fn create_backup(store_path: &Path) -> Result<u64, IoError> {
    let file_exists = fs::exists(store_path).map_err(|e| IoError::BackupFailed {
        path: store_path.to_path_buf(),
        source: e,
    })?;

    if !file_exists {
        return Ok(0);
    }

    let backup_path = get_backup_path(store_path);
    let copy_result = fs::copy(store_path, &backup_path);

    match copy_result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Backup directory doesn't exist yet — create it and retry
            let backup_dir = get_backup_dir(store_path);
            fs::create_dir(&backup_dir).map_err(|e| IoError::BackupFailed {
                path: backup_dir,
                source: e,
            })?;
            create_backup(store_path)
        }
        Err(e) => Err(IoError::BackupFailed {
            path: backup_path,
            source: e,
        }),
        Ok(bytes) => Ok(bytes),
    }
}

/// Keep at most `max_backups` backup files, removing the oldest ones.
pub fn cleanup_old_backups(store_path: &Path, max_backups: usize) -> Result<(), IoError> {
    let backup_dir = get_backup_dir(store_path);
    let backup_dir_exists = fs::exists(&backup_dir).map_err(|e| IoError::CleanupFailed {
        dir: backup_dir.clone(),
        source: e,
    })?;

    if !backup_dir_exists {
        return Ok(());
    }

    let mut file_entries = fs::read_dir(&backup_dir)
        .map_err(|e| IoError::CleanupFailed {
            dir: backup_dir.clone(),
            source: e,
        })?
        .flatten()
        .filter(|entry| entry.metadata().map(|m| m.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    file_entries.sort();

    let number_of_files_to_delete = file_entries.len().saturating_sub(max_backups);
    if number_of_files_to_delete == 0 {
        return Ok(());
    }

    for file_path in &file_entries[0..number_of_files_to_delete] {
        fs::remove_file(file_path).map_err(|e| IoError::CleanupFailed {
            dir: backup_dir.clone(),
            source: e,
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_dir_is_sibling() {
        let store = Path::new("/tmp/myapp/store.json");
        assert_eq!(get_backup_dir(store), PathBuf::from("/tmp/myapp/backups"));
    }

    #[test]
    fn no_backup_when_source_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("nonexistent.json");
        let bytes = create_backup(&store_path).unwrap();
        assert_eq!(bytes, 0);
    }

    #[test]
    fn creates_backup_and_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("store.json");

        for i in 0..7 {
            fs::write(&store_path, format!("content-{}", i)).unwrap();
            create_backup(&store_path).unwrap();
            cleanup_old_backups(&store_path, 5).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let backup_dir = get_backup_dir(&store_path);
        let count = fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.metadata().map(|m| m.is_file()).unwrap_or(false))
            .count();

        assert_eq!(count, 5);
    }
}
