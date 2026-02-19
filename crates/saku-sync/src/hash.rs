use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::SyncError;

/// Compute the hex-encoded SHA-256 digest of a file's contents.
pub fn sha256_file(path: &Path) -> Result<String, SyncError> {
    let data = std::fs::read(path)?;
    Ok(sha256_bytes(&data))
}

/// Compute the hex-encoded SHA-256 digest of a byte slice.
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn known_vector_empty() {
        // SHA-256 of empty input
        let hash = sha256_bytes(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn known_vector_hello() {
        let hash = sha256_bytes(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn file_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);

        let hash = sha256_file(&file_path).unwrap();
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn error_on_missing_file() {
        let result = sha256_file(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }
}
