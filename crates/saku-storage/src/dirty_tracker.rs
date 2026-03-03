use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::IoError;
use crate::io::atomic_writer::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirtyTracker {
    pub dirty_keys: HashSet<String>,
    pub last_cookie: Option<String>,
}

impl DirtyTracker {
    pub fn new() -> Self {
        Self {
            dirty_keys: HashSet::new(),
            last_cookie: None,
        }
    }

    /// Load from disk. Missing file → empty tracker. Corrupt file → empty tracker.
    /// Re-pushing is safe via LWW, so losing dirty state is acceptable.
    pub fn load(path: &Path) -> Result<Self, IoError> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(Self::new()),
        };

        match serde_json::from_str(&content) {
            Ok(tracker) => Ok(tracker),
            Err(_) => Ok(Self::new()),
        }
    }

    /// Persist to disk using atomic write (write to temp + rename).
    pub fn save(&self, path: &Path) -> Result<(), IoError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| IoError::WriteFailed {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e),
            })?;
        atomic_write(path, &json)
    }

    pub fn mark_dirty(&mut self, key: &str) {
        self.dirty_keys.insert(key.to_string());
    }

    pub fn mark_dirty_many(&mut self, keys: impl IntoIterator<Item = impl AsRef<str>>) {
        for key in keys {
            self.dirty_keys.insert(key.as_ref().to_string());
        }
    }

    pub fn clear_dirty(&mut self, keys: &[String]) {
        for key in keys {
            self.dirty_keys.remove(key);
        }
    }

    pub fn clear_all(&mut self) {
        self.dirty_keys.clear();
    }

    pub fn set_cookie(&mut self, cookie: Option<String>) {
        self.last_cookie = cookie;
    }

    pub fn is_dirty(&self, key: &str) -> bool {
        self.dirty_keys.contains(key)
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty_keys.len()
    }

    /// Given a store path like `/path/to/store.json`, returns `/path/to/dirty.json`.
    pub fn sidecar_path(store_path: &Path) -> PathBuf {
        store_path.with_file_name("dirty.json")
    }
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tracker_is_empty() {
        let tracker = DirtyTracker::new();
        assert!(tracker.dirty_keys.is_empty());
        assert!(tracker.last_cookie.is_none());
        assert_eq!(tracker.dirty_count(), 0);
    }

    #[test]
    fn mark_dirty_adds_key() {
        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty("task/abc123");
        assert!(tracker.is_dirty("task/abc123"));
        assert_eq!(tracker.dirty_count(), 1);
    }

    #[test]
    fn mark_dirty_is_idempotent() {
        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty("task/abc123");
        tracker.mark_dirty("task/abc123");
        assert_eq!(tracker.dirty_count(), 1);
    }

    #[test]
    fn mark_dirty_many() {
        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty_many(["task/a", "task/b", "project/c"]);
        assert_eq!(tracker.dirty_count(), 3);
        assert!(tracker.is_dirty("task/a"));
        assert!(tracker.is_dirty("task/b"));
        assert!(tracker.is_dirty("project/c"));
    }

    #[test]
    fn clear_dirty_removes_keys() {
        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty_many(["task/a", "task/b", "task/c"]);
        tracker.clear_dirty(&["task/a".to_string(), "task/c".to_string()]);
        assert_eq!(tracker.dirty_count(), 1);
        assert!(!tracker.is_dirty("task/a"));
        assert!(tracker.is_dirty("task/b"));
        assert!(!tracker.is_dirty("task/c"));
    }

    #[test]
    fn clear_all_empties_set() {
        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty_many(["task/a", "task/b"]);
        tracker.clear_all();
        assert_eq!(tracker.dirty_count(), 0);
        assert!(!tracker.is_dirty("task/a"));
    }

    #[test]
    fn set_cookie_updates_value() {
        let mut tracker = DirtyTracker::new();
        assert!(tracker.last_cookie.is_none());

        tracker.set_cookie(Some("42".to_string()));
        assert_eq!(tracker.last_cookie, Some("42".to_string()));

        tracker.set_cookie(None);
        assert!(tracker.last_cookie.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("dirty.json");

        let mut tracker = DirtyTracker::new();
        tracker.mark_dirty_many(["task/a", "project/b"]);
        tracker.set_cookie(Some("99".to_string()));
        tracker.save(&path).unwrap();

        let loaded = DirtyTracker::load(&path).unwrap();
        assert_eq!(loaded.dirty_count(), 2);
        assert!(loaded.is_dirty("task/a"));
        assert!(loaded.is_dirty("project/b"));
        assert_eq!(loaded.last_cookie, Some("99".to_string()));
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let tracker = DirtyTracker::load(&path).unwrap();
        assert!(tracker.dirty_keys.is_empty());
        assert!(tracker.last_cookie.is_none());
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("dirty.json");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let tracker = DirtyTracker::load(&path).unwrap();
        assert!(tracker.dirty_keys.is_empty());
        assert!(tracker.last_cookie.is_none());
    }

    #[test]
    fn sidecar_path_from_store_path() {
        let store = Path::new("/home/user/.local/share/saku/store.json");
        let dirty = DirtyTracker::sidecar_path(store);
        assert_eq!(dirty, Path::new("/home/user/.local/share/saku/dirty.json"));
    }
}
