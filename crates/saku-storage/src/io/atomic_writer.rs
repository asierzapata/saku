use std::path::Path;

use crate::error::IoError;

/// Atomically write content to a file by writing to a temp file first, then renaming.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), IoError> {
    let unique_temp = format!("{}.tmp.{}", path.display(), uuid::Uuid::new_v4());
    let temp_path = std::path::PathBuf::from(&unique_temp);

    std::fs::write(&temp_path, content).map_err(|e| IoError::WriteFailed {
        path: temp_path.clone(),
        source: e,
    })?;

    std::fs::rename(&temp_path, path).map_err(|e| IoError::RenameFailed {
        from: temp_path,
        to: path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.json");

        atomic_write(&file_path, r#"{"hello": "world"}"#).unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, r#"{"hello": "world"}"#);
    }

    #[test]
    fn overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.json");

        atomic_write(&file_path, "first").unwrap();
        atomic_write(&file_path, "second").unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "second");
    }
}
