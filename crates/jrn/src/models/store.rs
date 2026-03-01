use saku_storage::entity::Entity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::models::entry::{Entry, EntryKind};

pub const CURRENT_VERSION: u32 = 1;

/// On-disk format: { version: 1, entries: { "entry/k7m2a3x9": {...}, ... } }
#[derive(Serialize, Deserialize)]
pub struct StoredStore {
    pub version: u32,
    pub entries: HashMap<String, Value>,
}

impl Default for StoredStore {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: HashMap::new(),
        }
    }
}

/// In-memory representation
pub struct Store {
    pub version: u32,
    pub entries: HashMap<String, Entry>,
    /// Secondary index: entry_number -> storage_key for O(1) lookup
    pub entry_number_index: HashMap<u64, String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: HashMap::new(),
            entry_number_index: HashMap::new(),
        }
    }
}

impl Store {
    pub fn from_stored(stored: StoredStore) -> Self {
        let mut entries = HashMap::new();
        let mut entry_number_index = HashMap::new();

        for (key, value) in stored.entries {
            if key.starts_with("entry/") {
                if let Ok(entry) = serde_json::from_value::<Entry>(value) {
                    entry_number_index.insert(entry.entry_number, key.clone());
                    entries.insert(key, entry);
                }
            }
        }

        Self {
            version: stored.version,
            entries,
            entry_number_index,
        }
    }

    pub fn to_stored(&self) -> StoredStore {
        let mut entries = HashMap::new();
        for (key, entry) in &self.entries {
            if let Ok(value) = serde_json::to_value(entry) {
                entries.insert(key.clone(), value);
            }
        }
        StoredStore {
            version: self.version,
            entries,
        }
    }

    pub fn next_entry_number(&self) -> u64 {
        self.entries
            .values()
            .map(|e| e.entry_number)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn add_entry(&mut self, mut entry: Entry) {
        entry.entry_number = self.next_entry_number();
        let key = entry.storage_key();
        self.entry_number_index
            .insert(entry.entry_number, key.clone());
        self.entries.insert(key, entry);
    }

    pub fn get_entry_by_number(&self, number: u64) -> Option<&Entry> {
        let key = self.entry_number_index.get(&number)?;
        self.entries.get(key)
    }

    /// Get all active (non-deleted) entries
    pub fn get_active_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values().filter(|e| e.deleted_at.is_none())
    }

    /// Get entries for a specific date (ISO date string comparison), sorted by time
    pub fn get_entries_for_date(&self, date: &str) -> Vec<&Entry> {
        let mut entries: Vec<_> = self
            .get_active_entries()
            .filter(|e| e.date == date)
            .collect();
        entries.sort_by(|a, b| a.time.cmp(&b.time));
        entries
    }

    /// Get the most recent handoff entry (active, not deleted)
    pub fn get_latest_handoff(&self) -> Option<&Entry> {
        self.get_active_entries()
            .filter(|e| e.kind == EntryKind::Handoff)
            .max_by(|a, b| a.date.cmp(&b.date).then(a.time.cmp(&b.time)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::entry::Entry;

    fn make_entry(body: &str, date: &str, time: &str, kind: EntryKind) -> Entry {
        Entry {
            storage_key_suffix: format!("key_{}", body.replace(' ', "_")),
            body: body.into(),
            date: date.into(),
            time: time.into(),
            kind,
            ..Entry::default()
        }
    }

    #[test]
    fn from_stored_to_stored_roundtrip() {
        let mut store = Store::default();
        store.add_entry(make_entry("test entry", "2026-03-01", "10:00:00", EntryKind::Log));

        let stored = store.to_stored();
        let restored = Store::from_stored(stored);

        assert_eq!(restored.entries.len(), 1);
        let entry = restored.get_entry_by_number(1).unwrap();
        assert_eq!(entry.body, "test entry");
    }

    #[test]
    fn add_entry_auto_increments() {
        let mut store = Store::default();
        store.add_entry(make_entry("first", "2026-03-01", "10:00:00", EntryKind::Log));
        store.add_entry(make_entry("second", "2026-03-01", "11:00:00", EntryKind::Log));

        assert_eq!(store.get_entry_by_number(1).unwrap().body, "first");
        assert_eq!(store.get_entry_by_number(2).unwrap().body, "second");
        assert_eq!(store.next_entry_number(), 3);
    }

    #[test]
    fn get_entry_by_number_not_found() {
        let store = Store::default();
        assert!(store.get_entry_by_number(1).is_none());
        assert!(store.get_entry_by_number(999).is_none());
    }

    #[test]
    fn next_entry_number_empty_store() {
        let store = Store::default();
        assert_eq!(store.next_entry_number(), 1);
    }

    #[test]
    fn get_entries_for_date_sorted_by_time() {
        let mut store = Store::default();
        store.add_entry(make_entry("afternoon", "2026-03-01", "14:00:00", EntryKind::Log));
        store.add_entry(make_entry("morning", "2026-03-01", "09:00:00", EntryKind::Log));
        store.add_entry(make_entry("other day", "2026-03-02", "10:00:00", EntryKind::Log));

        let today = store.get_entries_for_date("2026-03-01");
        assert_eq!(today.len(), 2);
        assert_eq!(today[0].body, "morning");
        assert_eq!(today[1].body, "afternoon");
    }

    #[test]
    fn get_latest_handoff() {
        let mut store = Store::default();
        store.add_entry(make_entry("log entry", "2026-03-01", "10:00:00", EntryKind::Log));
        store.add_entry(make_entry("old handoff", "2026-03-01", "11:00:00", EntryKind::Handoff));
        store.add_entry(make_entry("new handoff", "2026-03-02", "09:00:00", EntryKind::Handoff));

        let latest = store.get_latest_handoff().unwrap();
        assert_eq!(latest.body, "new handoff");
    }

    #[test]
    fn get_latest_handoff_none_when_empty() {
        let store = Store::default();
        assert!(store.get_latest_handoff().is_none());
    }

    #[test]
    fn deleted_entries_excluded() {
        let mut store = Store::default();
        let mut entry = make_entry("deleted", "2026-03-01", "10:00:00", EntryKind::Log);
        entry.deleted_at = Some("2026-03-01T12:00:00Z".into());
        store.add_entry(entry);
        store.add_entry(make_entry("active", "2026-03-01", "11:00:00", EntryKind::Log));

        let today = store.get_entries_for_date("2026-03-01");
        assert_eq!(today.len(), 1);
        assert_eq!(today[0].body, "active");
    }
}
