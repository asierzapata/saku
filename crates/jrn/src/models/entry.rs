use saku_storage::entity::Entity;
use saku_storage::timestamp::HybridTimestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Log,
    Handoff,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::Log => write!(f, "Log"),
            EntryKind::Handoff => write!(f, "Handoff"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Short hash suffix used as the storage key (e.g., "k7m2a3x9")
    pub storage_key_suffix: String,
    /// User-facing auto-incremental entry number
    pub entry_number: u64,
    /// Entry content
    pub body: String,
    /// ISO date "2026-02-28"
    pub date: String,
    /// "HH:MM:SS"
    pub time: String,
    /// log or handoff
    pub kind: EntryKind,
    /// "human" or agent name
    pub author: String,
    /// e.g. "project/website"
    pub project_key: Option<String>,
    /// Tags on this entry
    pub tags: Vec<String>,
    /// Cross-tool refs like "tdo:42"
    pub refs: Vec<String>,
    /// ISO 8601 creation timestamp
    pub created_at: String,
    /// Hybrid timestamp for sync conflict resolution
    pub modified_at: HybridTimestamp,
    /// Soft-delete timestamp
    pub deleted_at: Option<String>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            storage_key_suffix: String::new(),
            entry_number: 0,
            body: String::new(),
            date: String::new(),
            time: String::new(),
            kind: EntryKind::Log,
            author: "human".to_string(),
            project_key: None,
            tags: vec![],
            refs: vec![],
            created_at: String::new(),
            modified_at: HybridTimestamp::default(),
            deleted_at: None,
        }
    }
}

impl Entity for Entry {
    fn entity_type() -> &'static str {
        "entry"
    }

    fn natural_key(&self) -> String {
        self.storage_key_suffix.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_is_entry() {
        assert_eq!(Entry::entity_type(), "entry");
    }

    #[test]
    fn storage_key_combines_type_and_suffix() {
        let entry = Entry {
            storage_key_suffix: "k7m2a3x9".into(),
            ..Entry::default()
        };
        assert_eq!(entry.storage_key(), "entry/k7m2a3x9");
    }

    #[test]
    fn serde_roundtrip() {
        let entry = Entry {
            storage_key_suffix: "abc123".into(),
            entry_number: 5,
            body: "Test entry".into(),
            date: "2026-03-01".into(),
            time: "14:30:00".into(),
            kind: EntryKind::Handoff,
            author: "claude".into(),
            project_key: Some("project/website".into()),
            tags: vec!["deploy".into()],
            refs: vec!["tdo:42".into()],
            created_at: "2026-03-01T14:30:00Z".into(),
            modified_at: HybridTimestamp::default(),
            deleted_at: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: Entry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.storage_key_suffix, "abc123");
        assert_eq!(deserialized.entry_number, 5);
        assert_eq!(deserialized.body, "Test entry");
        assert_eq!(deserialized.kind, EntryKind::Handoff);
        assert_eq!(deserialized.author, "claude");
        assert_eq!(deserialized.project_key, Some("project/website".into()));
        assert_eq!(deserialized.tags, vec!["deploy"]);
        assert_eq!(deserialized.refs, vec!["tdo:42"]);
    }

    #[test]
    fn entry_kind_serde() {
        let log_json = serde_json::to_string(&EntryKind::Log).unwrap();
        assert_eq!(log_json, "\"log\"");

        let handoff_json = serde_json::to_string(&EntryKind::Handoff).unwrap();
        assert_eq!(handoff_json, "\"handoff\"");

        let log: EntryKind = serde_json::from_str("\"log\"").unwrap();
        assert_eq!(log, EntryKind::Log);

        let handoff: EntryKind = serde_json::from_str("\"handoff\"").unwrap();
        assert_eq!(handoff, EntryKind::Handoff);
    }
}
