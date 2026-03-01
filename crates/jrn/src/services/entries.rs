use saku_storage::key_gen::generate_task_key;
use thiserror::Error;

use crate::{
    models::{
        entry::{Entry, EntryKind},
        store::Store,
    },
    storage::{Storage, StorageError},
};

#[derive(Debug, Error)]
pub enum AddEntryError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
pub struct AddEntryParameters {
    pub body: String,
    pub kind: EntryKind,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub refs: Vec<String>,
}

pub fn add_entry(
    store: &mut Store,
    storage: &impl Storage,
    parameters: AddEntryParameters,
) -> Result<Entry, AddEntryError> {
    let now = jiff::Zoned::now();
    let now_ms = jiff::Timestamp::now().as_millisecond();
    let device_id =
        saku_storage::device::get_or_create_device_id().unwrap_or_else(|_| "unknown".to_string());
    let key_suffix = generate_task_key(&device_id, now_ms);

    let author = detect_author();

    let project_key = parameters.project.map(|p| {
        if p.starts_with("project/") {
            p
        } else {
            format!("project/{}", p.to_lowercase())
        }
    });

    let date_str = now.strftime("%Y-%m-%d").to_string();
    let time_str = now.strftime("%H:%M:%S").to_string();
    let created_at = jiff::Timestamp::now().to_string();

    let entry = Entry {
        storage_key_suffix: key_suffix,
        entry_number: 0, // assigned by store.add_entry
        body: parameters.body,
        date: date_str,
        time: time_str,
        kind: parameters.kind,
        author,
        project_key,
        tags: parameters.tags,
        refs: parameters.refs,
        created_at,
        modified_at: crate::sync_clock::next_modified_at(),
        deleted_at: None,
    };

    store.add_entry(entry);

    storage.save(store)?;

    // Return the entry (now has entry_number assigned)
    let number = store.next_entry_number() - 1;
    Ok(store.get_entry_by_number(number).unwrap().clone())
}

/// Detect author from environment.
///
/// Priority: JRN_AUTHOR env var > CLAUDE_CODE env presence > "human"
pub fn detect_author() -> String {
    if let Ok(author) = std::env::var("JRN_AUTHOR") {
        if !author.is_empty() {
            return author;
        }
    }
    if std::env::var("CLAUDE_CODE").is_ok() {
        return "claude".to_string();
    }
    "human".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::json::JsonFileStorage;

    #[test]
    fn add_entry_assigns_number_and_saves() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("store.json");
        let storage = JsonFileStorage::new(store_path);

        let mut store = Store::default();

        let result = add_entry(
            &mut store,
            &storage,
            AddEntryParameters {
                body: "First entry".into(),
                kind: EntryKind::Log,
                project: None,
                tags: vec![],
                refs: vec![],
            },
        );

        let entry = result.unwrap();
        assert_eq!(entry.entry_number, 1);
        assert_eq!(entry.body, "First entry");
        assert_eq!(entry.kind, EntryKind::Log);

        // Verify saved to disk
        let loaded = storage.load().unwrap();
        assert_eq!(loaded.entries.len(), 1);
    }

    #[test]
    fn add_entry_with_project() {
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("store.json");
        let storage = JsonFileStorage::new(store_path);

        let mut store = Store::default();

        let entry = add_entry(
            &mut store,
            &storage,
            AddEntryParameters {
                body: "Working on website".into(),
                kind: EntryKind::Log,
                project: Some("Website".into()),
                tags: vec!["deploy".into()],
                refs: vec!["tdo:42".into()],
            },
        )
        .unwrap();

        assert_eq!(entry.project_key, Some("project/website".into()));
        assert_eq!(entry.tags, vec!["deploy"]);
        assert_eq!(entry.refs, vec!["tdo:42"]);
    }

    #[test]
    fn detect_author_default_is_human() {
        // Clear env vars that might interfere
        // SAFETY: This test runs single-threaded; no concurrent env access.
        unsafe {
            std::env::remove_var("JRN_AUTHOR");
            std::env::remove_var("CLAUDE_CODE");
        }
        assert_eq!(detect_author(), "human");
    }
}
