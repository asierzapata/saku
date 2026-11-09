use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use saku_storage::entity::Entity;
use saku_storage::kv_store::{self, KvStore};
use saku_sync::{
    SyncError,
    replica::{PendingEntry, Replica},
};
use serde::Serialize;
use serde_json::Value;

use crate::models::{
    area::Area,
    project::Project,
    store::{CURRENT_VERSION, Store, StoredStore},
    task::{Task, When},
};
use crate::storage::{Storage, StorageError};

const APPLICATION_ID: i64 = 0x54444f31;
const DATABASE_VERSION: i64 = 2;

pub struct SqliteStorage {
    connection: Connection,
}

pub struct SqliteTransaction<'conn> {
    transaction: RefCell<Transaction<'conn>>,
}

pub enum CommandMode {
    Read,
    Write,
    Edit,
}

pub struct SqliteCommand<'conn> {
    inner: CommandStorage<'conn>,
}

enum CommandStorage<'conn> {
    Read(&'conn Connection),
    Write(SqliteTransaction<'conn>),
    Edit {
        connection: &'conn Connection,
        snapshot: Store,
        generation: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingChange {
    pub key: String,
    pub revision: i64,
    pub value: Option<Value>,
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let (application_id, version) = database_identity(&connection)?;
        if (application_id == 0 && version == 0)
            || (application_id == APPLICATION_ID && version == 1)
        {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (application_id, version) = database_identity(&tx)?;
            if application_id == 0 && version == 0 {
                let objects: i64 =
                    tx.query_row("SELECT COUNT(*) FROM sqlite_schema", [], |row| row.get(0))?;
                if objects != 0 {
                    return Err(StorageError::InvalidData(
                        "unrecognized nonempty SQLite database".into(),
                    ));
                }
                tx.execute_batch(
                    "CREATE TABLE store_metadata (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        data_version INTEGER NOT NULL,
                        generation INTEGER NOT NULL CHECK (typeof(generation) = 'integer' AND generation >= 0)
                    );
                    INSERT INTO store_metadata (id, data_version, generation) VALUES (1, 9, 0);
                    CREATE TABLE entries (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
                    CREATE TABLE pending_changes (
                        key TEXT PRIMARY KEY NOT NULL,
                        revision INTEGER NOT NULL CHECK (typeof(revision) = 'integer' AND revision > 0),
                        value TEXT
                    );
                    CREATE TABLE sync_state (id INTEGER PRIMARY KEY CHECK(id = 1), target TEXT NOT NULL, cookie TEXT);
                    PRAGMA application_id = 0x54444f31;
                    PRAGMA user_version = 2;",
                )?;
            } else if application_id == APPLICATION_ID && version == 1 {
                validate_metadata(&tx)?;
                tx.prepare("SELECT key, value FROM entries LIMIT 0")?;
                tx.prepare("SELECT key, revision, value FROM pending_changes LIMIT 0")?;
                tx.execute_batch(
                    "CREATE TABLE sync_state (id INTEGER PRIMARY KEY CHECK(id = 1), target TEXT NOT NULL, cookie TEXT);
                     PRAGMA user_version = 2;",
                )?;
            } else {
                validate_identity(application_id, version)?;
            }
            validate_metadata(&tx)?;
            validate_schema(&tx)?;
            tx.commit()?;
        } else {
            validate_identity(application_id, version)?;
            validate_metadata(&connection)?;
            validate_schema(&connection)?;
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self { connection })
    }

    pub fn load(&self) -> Result<Store, StorageError> {
        Ok(Store::from_stored(read_store(&self.connection)?))
    }

    pub fn command(&mut self, mode: CommandMode) -> Result<SqliteCommand<'_>, StorageError> {
        let inner = match mode {
            CommandMode::Read => CommandStorage::Read(&self.connection),
            CommandMode::Write => CommandStorage::Write(SqliteTransaction {
                transaction: RefCell::new(
                    self.connection
                        .transaction_with_behavior(TransactionBehavior::Immediate)?,
                ),
            }),
            CommandMode::Edit => {
                let tx = self.connection.transaction()?;
                let snapshot = Store::from_stored(read_store(&tx)?);
                let generation = generation(&tx)?;
                tx.commit()?;
                CommandStorage::Edit {
                    connection: &self.connection,
                    snapshot,
                    generation,
                }
            }
        };
        Ok(SqliteCommand { inner })
    }

    pub fn is_pristine(&self) -> Result<bool, StorageError> {
        is_pristine(&self.connection)
    }

    pub fn existing_is_pristine(path: impl AsRef<Path>) -> Result<bool, StorageError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let (application_id, version) = database_identity(&connection)?;
        if application_id == 0 && version == 0 {
            let count: i64 =
                connection.query_row("SELECT COUNT(*) FROM sqlite_schema", [], |row| row.get(0))?;
            if count != 0 {
                return Err(StorageError::InvalidData(
                    "unrecognized nonempty SQLite database".into(),
                ));
            }
            return Ok(true);
        }
        if application_id == APPLICATION_ID && version == 1 {
            validate_metadata(&connection)?;
            return Ok(connection.query_row(
                "SELECT generation = 0 AND NOT EXISTS(SELECT 1 FROM entries) AND NOT EXISTS(SELECT 1 FROM pending_changes) FROM store_metadata WHERE id = 1",
                [],
                |row| row.get(0),
            )?);
        }
        validate_identity(application_id, version)?;
        validate_schema(&connection)?;
        is_pristine(&connection)
    }

    pub fn transaction<R, E>(
        &mut self,
        operation: impl FnOnce(&mut Store, &SqliteTransaction<'_>) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<StorageError>,
    {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::from)?;
        let storage = SqliteTransaction {
            transaction: RefCell::new(tx),
        };
        let mut store = storage.load()?;
        let result = operation(&mut store, &storage)?;
        storage.save(&store)?;
        storage
            .transaction
            .into_inner()
            .commit()
            .map_err(StorageError::from)?;
        Ok(result)
    }

    pub fn import_json(&mut self, json: &str) -> Result<(), StorageError> {
        let value = parse_json(json)?;
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| StorageError::InvalidData("invalid data version".into()))?;
        validate_version(version)?;
        check_fields(&value, &["version", "entries"], "store")?;
        let mut stored: StoredStore = serde_json::from_value(value)
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        validate_store(&stored)?;
        normalize_schedules(&mut stored.entries)?;
        let serialized = serialize_entries(&stored.entries)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_metadata(&tx)?;
        if !is_pristine(&tx)? {
            return Err(StorageError::InvalidData(
                "import requires a pristine database".into(),
            ));
        }
        tx.execute("UPDATE store_metadata SET generation = 1 WHERE id = 1", [])?;
        if !serialized.is_empty() {
            for (key, value) in serialized {
                tx.execute(
                    "INSERT INTO entries (key, value) VALUES (?1, ?2)",
                    params![key, value],
                )?;
                tx.execute(
                    "INSERT INTO pending_changes (key, revision, value) VALUES (?1, 1, ?2)",
                    params![key, value],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn export_json(&self) -> Result<String, StorageError> {
        serde_json::to_string_pretty(&read_store(&self.connection)?)
            .map_err(|source| StorageError::SerializeFailed { source })
    }

    pub fn pending_changes(&self) -> Result<Vec<PendingChange>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT key, revision, value FROM pending_changes ORDER BY key")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut changes = Vec::new();
        for row in rows {
            let (key, revision, value) = row?;
            if revision <= 0 {
                return Err(StorageError::InvalidData("invalid pending revision".into()));
            }
            if !(key
                .strip_prefix("task/")
                .is_some_and(|suffix| !suffix.is_empty())
                || key.starts_with("project/")
                || key.starts_with("area/"))
            {
                return Err(StorageError::InvalidData(format!(
                    "invalid pending key {key}"
                )));
            }
            let value = value.map(|value| parse_json(&value)).transpose()?;
            if let Some(value) = &value {
                validate_entry(&key, value, &mut HashSet::new())?;
            }
            changes.push(PendingChange {
                key,
                revision,
                value,
            });
        }
        Ok(changes)
    }

    pub fn acknowledge_changes(&mut self, changes: &[(String, i64)]) -> Result<(), StorageError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (key, revision) in changes {
            tx.execute(
                "DELETE FROM pending_changes WHERE key = ?1 AND revision = ?2",
                params![key, revision],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

impl SqliteTransaction<'_> {
    pub fn commit(self) -> Result<(), StorageError> {
        self.transaction.into_inner().commit()?;
        Ok(())
    }
}

impl SqliteCommand<'_> {
    pub fn commit(self) -> Result<(), StorageError> {
        match self.inner {
            CommandStorage::Write(tx) => tx.commit(),
            CommandStorage::Read(_) | CommandStorage::Edit { .. } => Ok(()),
        }
    }
}

impl Storage for SqliteCommand<'_> {
    fn load(&self) -> Result<Store, StorageError> {
        match &self.inner {
            CommandStorage::Read(connection) => Ok(Store::from_stored(read_store(connection)?)),
            CommandStorage::Write(tx) => tx.load(),
            CommandStorage::Edit { snapshot, .. } => Ok(snapshot.clone()),
        }
    }

    fn save(&self, store: &Store) -> Result<(), StorageError> {
        match &self.inner {
            CommandStorage::Read(_) => Err(StorageError::InvalidData("read-only command".into())),
            CommandStorage::Write(tx) => tx.save(store),
            CommandStorage::Edit {
                connection,
                generation: expected,
                ..
            } => {
                let tx = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
                if generation(&tx)? != *expected {
                    return Err(StorageError::InvalidData(
                        "store changed while editing; reopen the editor".into(),
                    ));
                }
                let tx = SqliteTransaction {
                    transaction: RefCell::new(tx),
                };
                tx.save(store)?;
                tx.commit()
            }
        }
    }
}

fn sync_error(error: StorageError) -> SyncError {
    SyncError::Backend {
        message: format!("SQLite replica: {error}"),
    }
}

impl Replica for SqliteStorage {
    fn prepare_sync(&mut self, target: &str) -> Result<Option<String>, SyncError> {
        self.prepare_sync_inner(target).map_err(sync_error)
    }

    fn apply_pull(
        &mut self,
        target: &str,
        expected_cookie: Option<&str>,
        cookie: &str,
        entries: &[(String, String)],
    ) -> Result<(), SyncError> {
        self.apply_pull_inner(target, expected_cookie, cookie, entries)
            .map_err(sync_error)
    }

    fn pending(&self) -> Result<Vec<PendingEntry>, SyncError> {
        Ok(self
            .pending_changes()
            .map_err(sync_error)?
            .into_iter()
            .map(|entry| PendingEntry {
                key: entry.key,
                revision: entry.revision,
                value: entry.value,
            })
            .collect())
    }

    fn acknowledge(&mut self, changes: &[(String, i64)]) -> Result<(), SyncError> {
        self.acknowledge_changes(changes).map_err(sync_error)
    }
}

impl SqliteStorage {
    fn prepare_sync_inner(&mut self, target: &str) -> Result<Option<String>, StorageError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT target, cookie FROM sync_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((bound, cookie)) = binding {
            if bound != target {
                return Err(StorageError::InvalidData(
                    "sync target changed; export/import into a separate data directory".into(),
                ));
            }
            tx.commit()?;
            return Ok(cookie);
        }
        let stored = read_store(&tx)?;
        let next = generation(&tx)?
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidData("generation overflow".into()))?;
        let values = serialize_entries(&stored.entries)?;
        tx.execute(
            "UPDATE store_metadata SET generation = ?1 WHERE id = 1",
            [next],
        )?;
        for (key, value) in values {
            tx.execute(
                "INSERT INTO pending_changes (key, revision, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET revision = excluded.revision, value = excluded.value
                 WHERE pending_changes.value IS NOT NULL",
                params![key, next, value],
            )?;
        }
        tx.execute(
            "INSERT INTO sync_state (id, target, cookie) VALUES (1, ?1, NULL)",
            [target],
        )?;
        tx.commit()?;
        Ok(None)
    }

    fn apply_pull_inner(
        &mut self,
        target: &str,
        expected_cookie: Option<&str>,
        cookie: &str,
        entries: &[(String, String)],
    ) -> Result<(), StorageError> {
        let mut remote = HashMap::new();
        for (key, text) in entries {
            let value = parse_json(text)?;
            validate_entry(key, &value, &mut HashSet::new())?;
            if remote.insert(key.clone(), value).is_some() {
                return Err(StorageError::InvalidData(format!(
                    "duplicate remote key {key}"
                )));
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT target, cookie FROM sync_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if binding
            .as_ref()
            .map(|(bound, cursor)| (bound.as_str(), cursor.as_deref()))
            != Some((target, expected_cookie))
        {
            return Err(StorageError::InvalidData(
                "sync target or cursor changed".into(),
            ));
        }
        let old = read_store(&tx)?;
        let pending = {
            let mut statement = tx.prepare("SELECT key, value FROM pending_changes")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            let mut values = HashMap::new();
            for row in rows {
                let (key, value) = row?;
                values.insert(key, value.map(|text| parse_json(&text)).transpose()?);
            }
            values
        };
        if remote.keys().any(|key| pending.get(key) == Some(&None)) {
            return Err(StorageError::InvalidData(
                "unsupported local hard deletion".into(),
            ));
        }
        let local = KvStore {
            version: old.version,
            entries: old.entries.clone(),
        };
        let mut merged = local;
        for (key, value) in &remote {
            if merged
                .entries
                .get(key)
                .is_none_or(|old| compare_sync_entries(key, value, old).is_gt())
            {
                merged.entries.insert(key.clone(), value.clone());
            }
        }
        kv_store::reconcile_renames(&mut merged);
        kv_store::repair_references(&mut merged, &saku_sync::conflict::tdo_entity_schemas());
        deduplicate_numbers(&mut merged.entries)?;
        validate_store(&StoredStore {
            version: old.version,
            entries: merged.entries.clone(),
        })?;
        normalize_schedules(&mut merged.entries)?;
        let result = StoredStore {
            version: old.version,
            entries: merged.entries,
        };
        validate_store(&result)?;
        let mut queues = Vec::new();
        let mut clears = Vec::new();
        let mut changed = Vec::new();
        for key in old
            .entries
            .keys()
            .chain(result.entries.keys())
            .collect::<HashSet<_>>()
        {
            let before = old.entries.get(key);
            let after = result.entries.get(key);
            let entry_changed = before != after;
            if entry_changed {
                changed.push((key.clone(), after.cloned()));
            }
            if let Some(remote_value) = remote.get(key) {
                if after == Some(remote_value) {
                    clears.push(key.clone());
                } else if let Some(value) = after {
                    if entry_changed || pending.get(key) != Some(&Some(value.clone())) {
                        queues.push((key.clone(), Some(value.clone())));
                    }
                }
            } else if entry_changed {
                queues.push((key.clone(), after.cloned()));
            }
        }
        let next = if !changed.is_empty() || !queues.is_empty() {
            Some(
                generation(&tx)?
                    .checked_add(1)
                    .ok_or_else(|| StorageError::InvalidData("generation overflow".into()))?,
            )
        } else {
            None
        };
        if let Some(next) = next {
            tx.execute(
                "UPDATE store_metadata SET generation = ?1 WHERE id = 1",
                [next],
            )?;
            for (key, value) in changed {
                if let Some(value) = value {
                    let value = serde_json::to_string(&value)
                        .map_err(|source| StorageError::SerializeFailed { source })?;
                    tx.execute("INSERT INTO entries (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
                } else {
                    tx.execute("DELETE FROM entries WHERE key = ?1", [key])?;
                }
            }
            for (key, value) in queues {
                let value = value
                    .map(|v| serde_json::to_string(&v))
                    .transpose()
                    .map_err(|source| StorageError::SerializeFailed { source })?;
                tx.execute("INSERT INTO pending_changes (key, revision, value) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET revision = excluded.revision, value = excluded.value", params![key, next, value])?;
            }
        }
        for key in clears {
            tx.execute("DELETE FROM pending_changes WHERE key = ?1", [key])?;
        }
        tx.execute("UPDATE sync_state SET cookie = ?1 WHERE id = 1", [cookie])?;
        tx.commit()?;
        Ok(())
    }
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn normalize_schedules(entries: &mut HashMap<String, Value>) -> Result<(), StorageError> {
    let today = jiff::Zoned::now().date();
    for (key, entry) in entries
        .iter_mut()
        .filter(|(key, _)| key.starts_with("task/"))
    {
        let kind = entry["when"]["type"].as_str();
        if matches!(kind, Some("Today" | "Anytime")) {
            let when: When = serde_json::from_value(entry["when"].clone())
                .map_err(|error| StorageError::InvalidData(format!("{key}: {error}")))?;
            let normalized = match when {
                When::LegacyToday { .. } => When::Scheduled { date: today },
                When::LegacyAnytime => When::Someday,
                _ => unreachable!(),
            };
            entry["when"] = serde_json::to_value(normalized)
                .map_err(|source| StorageError::SerializeFailed { source })?;
        }
    }
    Ok(())
}

fn has_stable_schedule(value: &Value) -> bool {
    !matches!(value["when"]["type"].as_str(), Some("Today" | "Anytime"))
}

fn compare_sync_entries(key: &str, left: &Value, right: &Value) -> std::cmp::Ordering {
    kv_store::compare_modified_at(left, right)
        .then_with(|| {
            if key.starts_with("task/") {
                has_stable_schedule(left).cmp(&has_stable_schedule(right))
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .then_with(|| {
            if key.starts_with("task/") {
                left["task_number"]
                    .as_u64()
                    .cmp(&right["task_number"].as_u64())
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .then_with(|| {
            canonical_json(left)
                .to_string()
                .cmp(&canonical_json(right).to_string())
        })
}

fn deduplicate_numbers(entries: &mut HashMap<String, Value>) -> Result<(), StorageError> {
    let mut groups: BTreeMap<u64, Vec<(String, jiff::Timestamp)>> = BTreeMap::new();
    for (key, value) in entries.iter().filter(|(key, _)| key.starts_with("task/")) {
        let task: Task = serde_json::from_value(value.clone())
            .map_err(|error| StorageError::InvalidData(format!("{key}: {error}")))?;
        groups
            .entry(task.task_number)
            .or_default()
            .push((key.clone(), task.created_at));
    }
    let mut max = groups.keys().next_back().copied().unwrap_or(0);
    for keys in groups.values_mut().filter(|keys| keys.len() > 1) {
        keys.sort_by(|(key_a, created_a), (key_b, created_b)| {
            created_a.cmp(created_b).then(key_a.cmp(key_b))
        });
        for (key, _) in &keys[1..] {
            max = max
                .checked_add(1)
                .ok_or_else(|| StorageError::InvalidData("task number exhausted".into()))?;
            entries.get_mut(key).unwrap()["task_number"] = Value::from(max);
        }
    }
    Ok(())
}

impl Storage for SqliteTransaction<'_> {
    fn load(&self) -> Result<Store, StorageError> {
        Ok(Store::from_stored(read_store(&self.transaction.borrow())?))
    }

    fn save(&self, store: &Store) -> Result<(), StorageError> {
        let mut new = serialize_store(store)?;
        let mut tx = self.transaction.borrow_mut();
        let old = read_store(&tx)?;
        let old_typed = serialize_store(&Store::from_stored(StoredStore {
            version: old.version,
            entries: old.entries.clone(),
        }))?;
        for (key, value) in &mut new.entries {
            if let (Some(raw), Some(typed)) = (old.entries.get(key), old_typed.entries.get(key)) {
                *value = merge_raw(raw, typed, value);
            }
        }
        validate_store(&new)?;
        let mut changes = Vec::new();
        for (key, value) in &new.entries {
            if old.entries.get(key) != Some(value) {
                changes.push((key.clone(), Some(value.clone())));
            }
        }
        for key in old.entries.keys() {
            if !new.entries.contains_key(key) {
                changes.push((key.clone(), None));
            }
        }
        if changes.is_empty() {
            return Ok(());
        }
        let changes: Vec<_> = changes
            .into_iter()
            .map(|(key, value)| {
                let value = value
                    .map(|value| {
                        serde_json::to_string(&value)
                            .map_err(|source| StorageError::SerializeFailed { source })
                    })
                    .transpose()?;
                Ok((key, value))
            })
            .collect::<Result<_, StorageError>>()?;
        let next = generation(&tx)?
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidData("generation overflow".into()))?;
        let savepoint = tx.savepoint()?;
        savepoint.execute(
            "UPDATE store_metadata SET generation = ?1 WHERE id = 1",
            [next],
        )?;
        for (key, value) in changes {
            if let Some(value) = &value {
                savepoint.execute(
                    "INSERT INTO entries (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
            } else {
                savepoint.execute("DELETE FROM entries WHERE key = ?1", [&key])?;
            }
            savepoint.execute(
                "INSERT INTO pending_changes (key, revision, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET revision = excluded.revision, value = excluded.value",
                params![key, next, value],
            )?;
        }
        savepoint.commit()?;
        Ok(())
    }
}

struct UniqueJsonValue(Value);

impl<'de> serde::Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;

        impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
            type Value = UniqueJsonValue;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Bool(value)))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Number(value.into())))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Number(value.into())))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJsonValue(Value::Number(number)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::String(value.into())))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::String(value)))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Null))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJsonValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJsonValue(Value::Array(values)))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key {key}"
                        )));
                    }
                    let UniqueJsonValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueJsonValue(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn parse_json(json: &str) -> Result<Value, StorageError> {
    serde_json::from_str::<UniqueJsonValue>(json)
        .map(|value| value.0)
        .map_err(|error| StorageError::InvalidData(error.to_string()))
}

fn database_identity(connection: &Connection) -> Result<(i64, i64), StorageError> {
    Ok(connection.query_row(
        "SELECT application_id, user_version FROM pragma_application_id, pragma_user_version",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

fn validate_identity(application_id: i64, version: i64) -> Result<(), StorageError> {
    if application_id != APPLICATION_ID {
        return Err(StorageError::InvalidData(
            "unrecognized SQLite application id".into(),
        ));
    }
    if version != DATABASE_VERSION {
        return Err(StorageError::UnsupportedDatabaseVersion(version));
    }
    Ok(())
}

fn validate_version(version: u32) -> Result<(), StorageError> {
    if version > CURRENT_VERSION {
        Err(StorageError::FutureVersion(version))
    } else if version < CURRENT_VERSION {
        Err(StorageError::UnsupportedVersion(version))
    } else {
        Ok(())
    }
}

fn validate_metadata(connection: &Connection) -> Result<(), StorageError> {
    let (version, generation): (i64, i64) = connection.query_row(
        "SELECT data_version, generation FROM store_metadata WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if generation < 0 {
        return Err(StorageError::InvalidData("invalid generation".into()));
    }
    let version = u32::try_from(version)
        .map_err(|_| StorageError::InvalidData("invalid data version".into()))?;
    validate_version(version)
}

fn validate_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.prepare("SELECT key, value FROM entries LIMIT 0")?;
    connection.prepare("SELECT key, revision, value FROM pending_changes LIMIT 0")?;
    connection.prepare("SELECT target, cookie FROM sync_state LIMIT 0")?;
    Ok(())
}

fn is_pristine(connection: &Connection) -> Result<bool, StorageError> {
    validate_metadata(connection)?;
    Ok(connection.query_row(
        "SELECT generation = 0 AND NOT EXISTS(SELECT 1 FROM entries) AND NOT EXISTS(SELECT 1 FROM pending_changes) AND NOT EXISTS(SELECT 1 FROM sync_state) FROM store_metadata WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}

fn generation(connection: &Connection) -> Result<i64, StorageError> {
    Ok(connection.query_row(
        "SELECT generation FROM store_metadata WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}

fn read_store(connection: &Connection) -> Result<StoredStore, StorageError> {
    validate_metadata(connection)?;
    let mut statement = connection.prepare("SELECT key, value FROM entries ORDER BY key")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut entries = HashMap::new();
    for row in rows {
        let (key, value) = row?;
        let value = parse_json(&value).map_err(|error| match error {
            StorageError::InvalidData(message) => {
                StorageError::InvalidData(format!("{key}: {message}"))
            }
            error => error,
        })?;
        if entries.insert(key.clone(), value).is_some() {
            return Err(StorageError::InvalidData(format!(
                "duplicate entry key {key}"
            )));
        }
    }
    let stored = StoredStore {
        version: CURRENT_VERSION,
        entries,
    };
    validate_store(&stored)?;
    Ok(stored)
}

fn serialize_entries(
    entries: &HashMap<String, Value>,
) -> Result<Vec<(String, String)>, StorageError> {
    entries
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                serde_json::to_string(value)
                    .map_err(|source| StorageError::SerializeFailed { source })?,
            ))
        })
        .collect()
}

fn serialize_store(store: &Store) -> Result<StoredStore, StorageError> {
    validate_version(store.version)?;
    let mut entries = HashMap::new();
    for (key, task) in &store.tasks {
        insert_serialized(&mut entries, key, task)?;
    }
    for (key, project) in &store.projects {
        insert_serialized(&mut entries, key, project)?;
    }
    for (key, area) in &store.areas {
        insert_serialized(&mut entries, key, area)?;
    }
    let stored = StoredStore {
        version: store.version,
        entries,
    };
    validate_store(&stored)?;
    Ok(stored)
}

fn insert_serialized<T: Serialize>(
    entries: &mut HashMap<String, Value>,
    key: &str,
    entity: &T,
) -> Result<(), StorageError> {
    let value =
        serde_json::to_value(entity).map_err(|source| StorageError::SerializeFailed { source })?;
    if entries.insert(key.to_owned(), value).is_some() {
        return Err(StorageError::InvalidData(format!("duplicate key {key}")));
    }
    Ok(())
}

fn validate_store(stored: &StoredStore) -> Result<(), StorageError> {
    validate_version(stored.version)?;
    let mut numbers = HashSet::new();
    for (key, value) in &stored.entries {
        validate_entry(key, value, &mut numbers)?;
    }
    Ok(())
}

fn check_fields(value: &Value, fields: &[&str], label: &str) -> Result<(), StorageError> {
    let object = value
        .as_object()
        .ok_or_else(|| StorageError::InvalidData(format!("{label} must be an object")))?;
    if let Some(field) = object
        .keys()
        .find(|field| !fields.contains(&field.as_str()))
    {
        return Err(StorageError::InvalidData(format!(
            "unknown field {field} in {label}"
        )));
    }
    Ok(())
}

fn validate_entry(
    key: &str,
    value: &Value,
    numbers: &mut HashSet<u64>,
) -> Result<(), StorageError> {
    let timestamp_fields = ["wall_ms", "lamport", "device_id"];
    if let Some(timestamp) = value.get("modified_at") {
        check_fields(timestamp, &timestamp_fields, "modified_at")?;
    }
    if key.starts_with("task/") {
        check_fields(
            value,
            &[
                "storage_key_suffix",
                "task_number",
                "title",
                "notes",
                "project_key",
                "area_key",
                "tags",
                "when",
                "deadline",
                "defer_until",
                "depends_on",
                "parent_task_key",
                "completed_at",
                "deleted_at",
                "created_at",
                "modified_at",
                "assigned_to",
                "estimate_minutes",
                "recurrence",
                "completed_occurrences",
            ],
            key,
        )?;
        if let Some(when) = value.get("when") {
            let allowed: &[&str] = match when.get("type").and_then(Value::as_str) {
                Some("Scheduled") => &["type", "date"],
                Some("Today") => &["type", "evening"],
                _ => &["type"],
            };
            check_fields(when, allowed, "when")?;
        }
        if let Some(recurrence) = value.get("recurrence").filter(|v| !v.is_null()) {
            check_fields(
                recurrence,
                &["freq", "weekdays", "monthly_anchor", "until", "dtstart"],
                "recurrence",
            )?;
            if let Some(anchor) = recurrence.get("monthly_anchor").filter(|v| !v.is_null()) {
                let allowed: &[&str] = match anchor.get("type").and_then(Value::as_str) {
                    Some("day_of_month") => &["type", "day"],
                    Some("nth_weekday") => &["type", "nth", "weekday"],
                    Some("last_weekday") => &["type", "weekday"],
                    _ => &["type"],
                };
                check_fields(anchor, allowed, "monthly_anchor")?;
            }
        }
        let task: Task = serde_json::from_value(value.clone())
            .map_err(|error| StorageError::InvalidData(format!("{key}: {error}")))?;
        if task.storage_key_suffix.is_empty() || task.storage_key() != key {
            return Err(StorageError::InvalidData(format!("invalid task key {key}")));
        }
        if task.task_number == 0 || !numbers.insert(task.task_number) {
            return Err(StorageError::InvalidData(format!(
                "invalid task number at {key}"
            )));
        }
    } else if key.starts_with("project/") {
        check_fields(
            value,
            &[
                "name",
                "area_key",
                "notes",
                "deadline",
                "completed_at",
                "deleted_at",
                "created_at",
                "modified_at",
                "renamed_to",
                "previous_key",
            ],
            key,
        )?;
        let project: Project = serde_json::from_value(value.clone())
            .map_err(|error| StorageError::InvalidData(format!("{key}: {error}")))?;
        if project.storage_key() != key {
            return Err(StorageError::InvalidData(format!(
                "invalid project key {key}"
            )));
        }
    } else if key.starts_with("area/") {
        check_fields(
            value,
            &[
                "name",
                "deleted_at",
                "modified_at",
                "renamed_to",
                "previous_key",
            ],
            key,
        )?;
        let area: Area = serde_json::from_value(value.clone())
            .map_err(|error| StorageError::InvalidData(format!("{key}: {error}")))?;
        if area.storage_key() != key {
            return Err(StorageError::InvalidData(format!("invalid area key {key}")));
        }
    } else {
        return Err(StorageError::InvalidData(format!(
            "unknown entity key {key}"
        )));
    }
    Ok(())
}

fn merge_raw(raw: &Value, old: &Value, new: &Value) -> Value {
    match (raw.as_object(), old.as_object(), new.as_object()) {
        (Some(raw), Some(old), Some(new)) => {
            if old.contains_key("type") && raw.get("type") != old.get("type") {
                return Value::Object(new.clone());
            }
            let mut result = raw.clone();
            for (key, old_value) in old {
                if !new.contains_key(key) {
                    result.remove(key);
                } else if let Some(raw_value) = raw.get(key) {
                    result.insert(key.clone(), merge_raw(raw_value, old_value, &new[key]));
                } else if old_value != &new[key] {
                    result.insert(key.clone(), new[key].clone());
                }
            }
            for (key, value) in new {
                if !old.contains_key(key) {
                    result.insert(key.clone(), value.clone());
                }
            }
            Value::Object(result)
        }
        _ if old == new => raw.clone(),
        _ => new.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        area::Area,
        project::Project,
        store::CURRENT_VERSION,
        task::{Freq, Recurrence, Task, When},
    };
    use crate::services::tasks::{
        AddTaskParameters, CompleteTaskParameters, add_task, complete_task,
    };
    use crate::storage::{Storage, StorageError};
    use jiff::civil::date;
    use rusqlite::{Connection, ErrorCode};
    use saku_storage::entity::Entity;
    use serde_json::{Value, json};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    fn task(suffix: &str) -> Task {
        Task {
            storage_key_suffix: suffix.into(),
            title: format!("Task {suffix}"),
            ..Task::default()
        }
    }

    fn generation(storage: &SqliteStorage) -> i64 {
        storage
            .connection
            .query_row(
                "SELECT generation FROM store_metadata WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn equal_clock_sync_order_is_numeric_canonical_and_symmetric() {
        let nine = json!({"task_number": 9, "title": "zzz", "modified_at": {"wall_ms": 1, "lamport": 1, "device_id": "d"}});
        let ten = json!({"task_number": 10, "title": "aaa", "modified_at": {"wall_ms": 1, "lamport": 1, "device_id": "d"}});
        assert!(compare_sync_entries("task/a", &ten, &nine).is_gt());
        assert!(compare_sync_entries("task/a", &nine, &ten).is_lt());
        let mut newer = nine.clone();
        newer["modified_at"]["wall_ms"] = json!(2);
        assert!(compare_sync_entries("task/a", &newer, &ten).is_gt());
        assert!(compare_sync_entries("task/a", &ten, &newer).is_lt());
        let left: Value =
            serde_json::from_str(r#"{"nested":{"a":1,"b":2},"other":[{"x":1,"y":2}]}"#).unwrap();
        let right: Value =
            serde_json::from_str(r#"{"other":[{"y":2,"x":1}],"nested":{"b":2,"a":1}}"#).unwrap();
        assert_eq!(
            compare_sync_entries("area/a", &left, &right),
            std::cmp::Ordering::Equal
        );
        let low = json!({"name":"a","modified_at":{"wall_ms":1,"lamport":0,"device_id":"d"}});
        let high = json!({"name":"z","modified_at":{"wall_ms":1,"lamport":0,"device_id":"d"}});
        assert!(compare_sync_entries("area/a", &high, &low).is_gt());
        assert!(compare_sync_entries("area/a", &low, &high).is_lt());
    }

    fn fixture() -> Value {
        let mut store = Store::default();
        store.add_area(Area {
            name: "Old Area".into(),
            deleted_at: Some(jiff::Timestamp::now()),
            renamed_to: Some("area/new area".into()),
            ..Area::default()
        });
        store.add_area(Area {
            name: "New Area".into(),
            previous_key: Some("area/old area".into()),
            ..Area::default()
        });
        store.add_project(Project {
            name: "Old Project".into(),
            deleted_at: Some(jiff::Timestamp::now()),
            renamed_to: Some("project/new project".into()),
            ..Project::default()
        });
        store.add_project(Project {
            name: "New Project".into(),
            area_key: Some("area/new area".into()),
            previous_key: Some("project/old project".into()),
            ..Project::default()
        });
        let mut parent = task("parent");
        parent.notes = Some("A note".into());
        parent.project_key = Some("project/new project".into());
        parent.when = When::Scheduled {
            date: date(2026, 3, 10),
        };
        parent.recurrence = Some(Recurrence {
            freq: Freq::Weekly,
            weekdays: vec![],
            monthly_anchor: None,
            until: None,
            dtstart: date(2026, 3, 10),
        });
        parent.depends_on = vec!["task/child".into()];
        store.add_task(parent);
        let mut child = task("child");
        child.parent_task_key = Some("task/parent".into());
        store.add_task(child);
        let mut value = serde_json::to_value(store.to_stored()).unwrap();
        value["entries"]["task/parent"]["assigned_to"] = Value::Null;
        value["entries"]["task/parent"]["estimate_minutes"] = Value::Null;
        value["entries"]["project/new project"]["renamed_to"] = Value::Null;
        value
    }

    #[test]
    fn empty_reopen_roundtrip_and_final_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let mut storage = SqliteStorage::open(&path).unwrap();
        assert!(storage.load().unwrap().tasks.is_empty());
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_area(Area {
                    name: "Work".into(),
                    ..Area::default()
                });
                store.add_project(Project {
                    name: "Project".into(),
                    ..Project::default()
                });
                store.add_task(task("one"));
                Ok(())
            })
            .unwrap();
        drop(storage);
        let storage = SqliteStorage::open(&path).unwrap();
        let store = storage.load().unwrap();
        assert_eq!(store.get_task_by_number(1).unwrap().title, "Task one");
        assert!(store.get_area("area/work").is_some());
        assert!(store.get_project("project/project").is_some());
        assert_eq!(storage.pending_changes().unwrap().len(), 3);
    }

    #[test]
    fn task_services_add_and_complete_inside_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage
            .transaction::<_, StorageError>(|store, tx| {
                add_task(
                    store,
                    tx,
                    AddTaskParameters {
                        title: "Service task".into(),
                        notes: None,
                        when: When::Inbox,
                        deadline: None,
                        defer_until: None,
                        project: None,
                        area: None,
                        tags: vec![],
                        recurrence: None,
                        parent_task_number: None,
                        estimate_minutes: None,
                    },
                )
                .map_err(|e| StorageError::InvalidData(e.to_string()))?;
                complete_task(
                    store,
                    tx,
                    CompleteTaskParameters {
                        task_number_or_fuzzy_name: "1".into(),
                        stop: false,
                        note: Some("done".into()),
                    },
                )
                .map_err(|e| StorageError::InvalidData(e.to_string()))?;
                Ok(())
            })
            .unwrap();
        let store = storage.load().unwrap();
        let completed = store.get_task_by_number(1).unwrap();
        assert!(completed.completed_at.is_some());
        assert!(completed.notes.as_deref().unwrap().contains("done"));
        let pending = storage.pending_changes().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].value,
            Some(serde_json::to_value(completed).unwrap())
        );
    }

    #[test]
    fn upgrade_v1_preserves_data_during_simultaneous_opens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let mut storage = SqliteStorage::open(&path).unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_task(task("before"));
                Ok(())
            })
            .unwrap();
        let before = storage.pending_changes().unwrap();
        storage
            .connection
            .execute_batch("DROP TABLE sync_state; PRAGMA user_version = 1;")
            .unwrap();
        drop(storage);
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    SqliteStorage::open(path).unwrap()
                })
            })
            .collect();
        for thread in threads {
            let opened = thread.join().unwrap();
            assert_eq!(opened.pending_changes().unwrap(), before);
            assert_eq!(opened.load().unwrap().tasks.len(), 1);
            assert_eq!(database_identity(&opened.connection).unwrap().1, 2);
        }
    }

    #[test]
    fn initial_sync_binding_consumes_empty_store_and_overflow_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("empty.sqlite")).unwrap();
        assert!(storage.is_pristine().unwrap());
        assert_eq!(storage.prepare_sync("target").unwrap(), None);
        assert!(!storage.is_pristine().unwrap());
        assert_eq!(generation(&storage), 1);
        assert!(
            storage
                .import_json(r#"{"version":9,"entries":{}}"#)
                .is_err()
        );
        let mut overflow = SqliteStorage::open(dir.path().join("overflow.sqlite")).unwrap();
        overflow
            .connection
            .execute(
                "UPDATE store_metadata SET generation = ?1 WHERE id = 1",
                [i64::MAX],
            )
            .unwrap();
        assert!(overflow.prepare_sync("target").is_err());
        let bindings: i64 = overflow
            .connection
            .query_row("SELECT COUNT(*) FROM sync_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bindings, 0);
        assert_eq!(generation(&overflow), i64::MAX);
        assert!(overflow.pending_changes().unwrap().is_empty());
    }

    #[test]
    fn add_task_uses_unique_keys_and_refuses_number_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let params = || AddTaskParameters {
            title: "rapid".into(),
            notes: None,
            when: When::Inbox,
            deadline: None,
            defer_until: None,
            project: None,
            area: None,
            tags: vec![],
            recurrence: None,
            parent_task_number: None,
            estimate_minutes: None,
        };
        storage
            .transaction::<_, StorageError>(|store, tx| {
                for _ in 0..50 {
                    add_task(store, tx, params())
                        .map_err(|e| StorageError::InvalidData(e.to_string()))?;
                }
                assert_eq!(store.tasks.len(), 50);
                Ok(())
            })
            .unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                let mut maximum = task("max");
                maximum.task_number = u64::MAX;
                store.tasks.insert("task/max".into(), maximum);
                Ok(())
            })
            .unwrap();
        let before = storage.pending_changes().unwrap();
        let result = storage.transaction::<_, StorageError>(|store, tx| {
            match add_task(store, tx, params()) {
                Err(crate::services::tasks::AddTaskError::Storage(StorageError::InvalidData(
                    message,
                ))) => assert_eq!(message, "task number exhausted"),
                _ => panic!("expected exhaustion error"),
            }
            Ok(())
        });
        result.unwrap();
        assert_eq!(storage.load().unwrap().tasks.len(), 51);
        assert_eq!(storage.pending_changes().unwrap(), before);
    }

    #[test]
    fn error_and_panic_roll_back_intermediate_saves() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_task(task("base"));
                Ok(())
            })
            .unwrap();
        let before = serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap();
        let pending = storage.pending_changes().unwrap();
        let initial_generation = generation(&storage);
        let result = storage.transaction::<(), StorageError>(|store, tx| {
            store.add_task(task("error"));
            tx.save(store)?;
            Err(StorageError::InvalidData("abort".into()))
        });
        assert!(matches!(result, Err(StorageError::InvalidData(_))));
        assert_eq!(
            serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
            before
        );
        assert_eq!(storage.pending_changes().unwrap(), pending);
        assert_eq!(generation(&storage), initial_generation);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = storage.transaction::<(), StorageError>(|store, tx| {
                store.add_task(task("panic"));
                tx.save(store)?;
                panic!("rollback test");
            });
        }));
        assert!(result.is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
            before
        );
        assert_eq!(storage.pending_changes().unwrap(), pending);
        assert_eq!(generation(&storage), initial_generation);
    }

    #[test]
    fn failed_outbox_insert_rolls_back_savepoint_even_when_caught() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage.connection.execute_batch(
            "CREATE TEMP TRIGGER fail_pending BEFORE INSERT ON pending_changes BEGIN SELECT RAISE(ABORT, 'injected pending failure'); END;"
        ).unwrap();
        let result = storage.transaction::<(), StorageError>(|store, tx| {
            store.add_task(task("one"));
            assert!(matches!(tx.save(store), Err(StorageError::Database(_))));
            assert!(tx.load()?.tasks.is_empty());
            Err(StorageError::InvalidData(
                "abort after checking savepoint".into(),
            ))
        });
        assert!(matches!(result, Err(StorageError::InvalidData(_))));
        assert!(storage.load().unwrap().tasks.is_empty());
        assert!(storage.pending_changes().unwrap().is_empty());
        assert_eq!(generation(&storage), 0);
    }

    #[test]
    fn failed_import_outbox_insert_rolls_back_everything() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage.connection.execute_batch(
            "CREATE TEMP TRIGGER fail_pending BEFORE INSERT ON pending_changes BEGIN SELECT RAISE(ABORT, 'injected pending failure'); END;"
        ).unwrap();
        assert!(matches!(
            storage.import_json(&fixture().to_string()),
            Err(StorageError::Database(_))
        ));
        assert!(storage.load().unwrap().tasks.is_empty());
        assert!(storage.load().unwrap().projects.is_empty());
        assert!(storage.load().unwrap().areas.is_empty());
        assert!(storage.pending_changes().unwrap().is_empty());
        assert_eq!(generation(&storage), 0);
    }

    #[test]
    fn writer_lock_precedes_load_and_closure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let mut first = SqliteStorage::open(&path).unwrap();
        let mut second = SqliteStorage::open(&path).unwrap();
        second.connection.busy_timeout(Duration::ZERO).unwrap();
        let mut ran = false;
        first.transaction::<_, StorageError>(|store, _| {
            store.add_task(task("first"));
            let result = second.transaction::<_, StorageError>(|_, _| {
                ran = true;
                Ok(())
            });
            assert!(matches!(result, Err(StorageError::Database(rusqlite::Error::SqliteFailure(e, _))) if e.code == ErrorCode::DatabaseBusy));
            Ok(())
        }).unwrap();
        assert!(!ran);
        second
            .transaction::<_, StorageError>(|store, _| {
                assert_eq!(
                    store.get_task_by_number(1).unwrap().storage_key_suffix,
                    "first"
                );
                assert_eq!(store.next_task_number(), 2);
                store.add_task(task("second"));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            second
                .load()
                .unwrap()
                .get_task_by_number(2)
                .unwrap()
                .storage_key_suffix,
            "second"
        );
    }

    #[test]
    fn simultaneous_fresh_opens_and_concurrent_writers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let start = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|thread_id| {
                let start = start.clone();
                let path = path.clone();
                std::thread::spawn(move || {
                    start.wait();
                    let mut storage = SqliteStorage::open(path).unwrap();
                    for index in 0..20 {
                        storage
                            .transaction::<_, StorageError>(|store, _| {
                                store.add_task(task(&format!("{thread_id}-{index}")));
                                Ok(())
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let storage = SqliteStorage::open(path).unwrap();
        let store = storage.load().unwrap();
        assert_eq!(store.tasks.len(), 40);
        for number in 1..=40 {
            assert!(store.get_task_by_number(number).is_some());
        }
        assert_eq!(storage.pending_changes().unwrap().len(), 40);
    }

    #[test]
    fn import_export_preserves_raw_json_and_bootstraps_pending() {
        let dir = tempfile::tempdir().unwrap();
        let source = serde_json::to_string_pretty(&fixture()).unwrap();
        let original = source.clone();
        let mut storage = SqliteStorage::open(dir.path().join("first.sqlite")).unwrap();
        storage.import_json(&source).unwrap();
        assert_eq!(source, original);
        let exported = storage.export_json().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&exported).unwrap(),
            fixture_from(&source)
        );
        let entries = fixture_from(&source)["entries"]
            .as_object()
            .unwrap()
            .clone();
        let pending = storage.pending_changes().unwrap();
        assert_eq!(pending.len(), entries.len());
        for change in pending {
            assert_eq!(change.revision, 1);
            assert_eq!(change.value.as_ref(), entries.get(&change.key));
        }
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
            fixture_from(&source)
        );
        assert_eq!(generation(&storage), 1);
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.get_mut("task/parent").unwrap().title = "changed".into();
                Ok(())
            })
            .unwrap();
        let edited: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let mut expected = fixture_from(&source);
        expected["entries"]["task/parent"]["title"] = json!("changed");
        assert_eq!(edited, expected);
        let mut restored = SqliteStorage::open(dir.path().join("second.sqlite")).unwrap();
        restored.import_json(&exported).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&restored.export_json().unwrap()).unwrap(),
            fixture_from(&source)
        );
    }

    fn fixture_from(source: &str) -> Value {
        serde_json::from_str(source).unwrap()
    }

    #[test]
    fn invalid_imports_do_not_write_anything() {
        let original = fixture();
        let mut cases: Vec<(&str, Value)> = Vec::new();
        let mut value = original.clone();
        value["extra"] = json!(true);
        cases.push(("top level", value));
        let mut value = original.clone();
        value["entries"]["unknown/id"] = json!({});
        cases.push(("unknown kind", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["unexpected"] = json!(1);
        cases.push(("unknown field", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["recurrence"]["unexpected"] = json!(1);
        cases.push(("unknown nested field", value));
        let mut value = original.clone();
        value["entries"]["task/parent"] = json!("bad");
        cases.push(("malformed", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["storage_key_suffix"] = json!("wrong");
        cases.push(("key mismatch", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["storage_key_suffix"] = json!("");
        cases.push(("empty suffix", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["task_number"] = json!(0);
        cases.push(("zero", value));
        let mut value = original.clone();
        value["entries"]["task/parent"]["task_number"] = json!(2);
        cases.push(("duplicate", value));
        let mut value = original.clone();
        value["version"] = json!(CURRENT_VERSION + 1);
        value["future_field"] = json!("not understood");
        cases.push(("future", value));
        let mut value = original.clone();
        value["version"] = json!(CURRENT_VERSION - 1);
        cases.push(("old", value));
        for (name, value) in cases {
            let dir = tempfile::tempdir().unwrap();
            let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
            let result = storage.import_json(&value.to_string());
            match name {
                "future" => assert!(
                    matches!(result, Err(StorageError::FutureVersion(_))),
                    "{name}: {result:?}"
                ),
                "old" => assert!(
                    matches!(result, Err(StorageError::UnsupportedVersion(_))),
                    "{name}: {result:?}"
                ),
                _ => assert!(result.is_err(), "{name}"),
            }
            let stored = serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap();
            assert_eq!(stored["entries"], json!({}), "{name}");
            assert!(storage.pending_changes().unwrap().is_empty(), "{name}");
            assert_eq!(generation(&storage), 0, "{name}");
        }
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        assert!(storage.import_json("{invalid").is_err());
        assert_eq!(generation(&storage), 0);
        storage.import_json(&original.to_string()).unwrap();
        let before = serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap();
        let pending = storage.pending_changes().unwrap();
        assert!(storage.import_json(&original.to_string()).is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap(),
            before
        );
        assert_eq!(storage.pending_changes().unwrap(), pending);
    }

    #[test]
    fn pending_noop_update_remove_recreate_and_stale_ack() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_task(task("a"));
                store.add_task(task("b"));
                Ok(())
            })
            .unwrap();
        let initial = storage.pending_changes().unwrap();
        assert_eq!(initial.len(), 2);
        assert!(initial.iter().all(|c| c.revision == 1));
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(generation(&storage), 1);
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.get_mut("task/a").unwrap().title = "changed".into();
                Ok(())
            })
            .unwrap();
        let updated = storage.pending_changes().unwrap();
        assert_eq!(updated[0].revision, 2);
        assert_eq!(updated[0].value.as_ref().unwrap()["title"], "changed");
        assert_eq!(updated[1].revision, 1);
        storage
            .acknowledge_changes(&[("task/a".into(), 1), ("task/b".into(), 1)])
            .unwrap();
        assert_eq!(storage.pending_changes().unwrap(), vec![updated[0].clone()]);
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.remove("task/a");
                Ok(())
            })
            .unwrap();
        let removed = storage.pending_changes().unwrap();
        assert_eq!(removed[0].revision, 3);
        assert_eq!(removed[0].value, None);
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_task(task("a"));
                Ok(())
            })
            .unwrap();
        let recreated = storage.pending_changes().unwrap();
        assert_eq!(recreated[0].revision, 4);
        assert!(recreated[0].value.is_some());
        storage
            .acknowledge_changes(&[("task/a".into(), 3)])
            .unwrap();
        assert_eq!(storage.pending_changes().unwrap(), recreated);
        storage
            .acknowledge_changes(&[("task/a".into(), 4)])
            .unwrap();
        assert!(storage.pending_changes().unwrap().is_empty());
        assert_eq!(generation(&storage), 4);
    }

    #[test]
    fn import_rejects_nonpristine_history_and_generation_overflow_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.add_task(task("a"));
                Ok(())
            })
            .unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.clear();
                Ok(())
            })
            .unwrap();
        let pending = storage.pending_changes().unwrap();
        storage
            .acknowledge_changes(&[(pending[0].key.clone(), pending[0].revision)])
            .unwrap();
        assert!(
            storage
                .import_json(r#"{"version":9,"entries":{}}"#)
                .is_err()
        );
        assert_eq!(generation(&storage), 2);
        storage
            .connection
            .execute("UPDATE store_metadata SET generation = ?1", [i64::MAX])
            .unwrap();
        let result = storage.transaction::<_, StorageError>(|store, _| {
            store.add_task(task("overflow"));
            Ok(())
        });
        assert!(result.is_err());
        assert!(storage.load().unwrap().tasks.is_empty());
        assert!(storage.pending_changes().unwrap().is_empty());
        assert_eq!(generation(&storage), i64::MAX);
    }

    #[test]
    fn reject_unrelated_future_and_invalid_metadata_without_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unrelated.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE sqlitex (value TEXT); INSERT INTO sqlitex VALUES ('keep');",
        )
        .unwrap();
        drop(conn);
        assert!(SqliteStorage::open(&path).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.query_row("SELECT value FROM sqlitex", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        let future = dir.path().join("future.sqlite");
        let conn = Connection::open(&future).unwrap();
        conn.execute_batch("PRAGMA application_id = 0x54444f31; PRAGMA user_version = 3;")
            .unwrap();
        drop(conn);
        assert!(matches!(
            SqliteStorage::open(&future),
            Err(StorageError::UnsupportedDatabaseVersion(3))
        ));
        let conn = Connection::open(&future).unwrap();
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        let old_data = dir.path().join("old-data.sqlite");
        let storage = SqliteStorage::open(&old_data).unwrap();
        storage
            .connection
            .execute("UPDATE store_metadata SET data_version = 8", [])
            .unwrap();
        drop(storage);
        assert!(matches!(
            SqliteStorage::open(&old_data),
            Err(StorageError::UnsupportedVersion(8))
        ));
        let future_data = dir.path().join("future-data.sqlite");
        let storage = SqliteStorage::open(&future_data).unwrap();
        storage
            .connection
            .execute("UPDATE store_metadata SET data_version = 10", [])
            .unwrap();
        drop(storage);
        assert!(matches!(
            SqliteStorage::open(&future_data),
            Err(StorageError::FutureVersion(10))
        ));
        let missing = dir.path().join("missing.sqlite");
        let storage = SqliteStorage::open(&missing).unwrap();
        storage
            .connection
            .execute("DELETE FROM store_metadata", [])
            .unwrap();
        drop(storage);
        assert!(SqliteStorage::open(&missing).is_err());
    }

    #[test]
    fn invalid_database_entry_is_not_silently_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage
            .connection
            .execute(
                "INSERT INTO entries (key, value) VALUES (?1, ?2)",
                ["task/bad", "{invalid"],
            )
            .unwrap();
        assert!(storage.load().is_err());
        assert!(storage.export_json().is_err());
    }

    #[test]
    fn invalid_pending_rows_fail_instead_of_being_returned() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        storage.connection.execute(
            "INSERT INTO pending_changes (key, revision, value) VALUES ('unknown/key', 1, NULL)",
            [],
        ).unwrap();
        assert!(storage.pending_changes().is_err());
        storage
            .connection
            .execute("DELETE FROM pending_changes", [])
            .unwrap();
        storage.connection.execute(
            "INSERT INTO pending_changes (key, revision, value) VALUES ('task/bad', 1, '{invalid')",
            [],
        ).unwrap();
        assert!(storage.pending_changes().is_err());
    }

    #[test]
    fn save_rejects_invalid_typed_stores_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        for case in ["future", "old", "key", "zero", "duplicate"] {
            let result = storage.transaction::<_, StorageError>(|store, _| {
                match case {
                    "future" => store.version = CURRENT_VERSION + 1,
                    "old" => store.version = CURRENT_VERSION - 1,
                    "key" => {
                        store.tasks.insert("task/wrong".into(), task("real"));
                    }
                    "zero" => {
                        store.tasks.insert("task/zero".into(), task("zero"));
                    }
                    "duplicate" => {
                        let mut first = task("a");
                        first.task_number = 1;
                        let mut second = task("b");
                        second.task_number = 1;
                        store.tasks.insert(first.storage_key(), first);
                        store.tasks.insert(second.storage_key(), second);
                    }
                    _ => unreachable!(),
                }
                Ok(())
            });
            assert!(result.is_err(), "{case}");
            assert!(storage.load().unwrap().tasks.is_empty(), "{case}");
            assert!(storage.pending_changes().unwrap().is_empty(), "{case}");
            assert_eq!(generation(&storage), 0, "{case}");
        }
    }

    #[test]
    fn changing_a_normalized_legacy_when_keeps_a_valid_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let mut value = fixture();
        value["entries"]["task/parent"]["when"] = json!({"type":"Today","evening":false});
        storage.import_json(&value.to_string()).unwrap();
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.get_mut("task/parent").unwrap().when = When::Scheduled {
                    date: date(2027, 1, 2),
                };
                Ok(())
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&storage.export_json().unwrap()).unwrap()["entries"]["task/parent"]
                ["when"],
            json!({"type":"Scheduled","date":"2027-01-02"})
        );
    }

    fn assert_duplicate_import_rejected(source: &str) {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        assert!(matches!(
            storage.import_json(source),
            Err(StorageError::InvalidData(_))
        ));
        let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        assert_eq!(exported["entries"], json!({}));
        assert!(storage.pending_changes().unwrap().is_empty());
        assert_eq!(generation(&storage), 0);
    }

    fn parent_json() -> String {
        serde_json::to_string(&fixture()["entries"]["task/parent"]).unwrap()
    }

    fn entry_json(value: &str) -> String {
        format!(r#"{{"version":9,"entries":{{"task/parent":{value}}}}}"#)
    }

    #[test]
    fn unique_json_parser_preserves_values_and_rejects_trailing_input() {
        let source = r#"[true,false,null,-3,18446744073709551615,1.25,["ok"],{"key":"value"}]"#;
        assert_eq!(
            parse_json(source).unwrap(),
            serde_json::from_str::<Value>(source).unwrap()
        );
        assert!(parse_json(r#"{"key":1} trailing"#).is_err());
        assert!(parse_json(r#"{"title":1,"\u0074itle":2}"#).is_err());
    }

    #[test]
    fn regression_duplicate_root_version_rejected() {
        assert_duplicate_import_rejected(r#"{"version":10,"version":9,"entries":{}}"#);
    }

    #[test]
    fn regression_duplicate_entry_key_rejected() {
        let parent = parent_json();
        let source = format!(
            r#"{{"version":9,"entries":{{"task/parent":{parent},"task/parent":{parent}}}}}"#
        );
        assert_duplicate_import_rejected(&source);
    }

    #[test]
    fn regression_duplicate_entity_field_rejected() {
        let parent = parent_json();
        let parent = format!(
            r#"{{"title":"discarded",{}"#,
            parent.strip_prefix('{').unwrap()
        );
        assert_duplicate_import_rejected(&entry_json(&parent));
    }

    #[test]
    fn regression_duplicate_nested_when_type_rejected() {
        let parent = parent_json();
        let duplicate = parent.replacen(
            r#""type":"Scheduled""#,
            r#""type":"Someday","type":"Scheduled""#,
            1,
        );
        assert_ne!(parent, duplicate);
        assert_duplicate_import_rejected(&entry_json(&duplicate));
    }

    #[test]
    fn regression_duplicate_database_entry_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let parent = parent_json();
        let duplicate = format!(
            r#"{{"title":"discarded",{}"#,
            parent.strip_prefix('{').unwrap()
        );
        storage
            .connection
            .execute(
                "INSERT INTO entries (key, value) VALUES (?1, ?2)",
                params!["task/parent", duplicate],
            )
            .unwrap();
        assert!(matches!(storage.load(), Err(StorageError::InvalidData(_))));
        assert!(matches!(
            storage.export_json(),
            Err(StorageError::InvalidData(_))
        ));
    }

    #[test]
    fn regression_duplicate_pending_value_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let parent = parent_json();
        let duplicate = format!(
            r#"{{"title":"discarded",{}"#,
            parent.strip_prefix('{').unwrap()
        );
        storage
            .connection
            .execute(
                "INSERT INTO pending_changes (key, revision, value) VALUES (?1, 1, ?2)",
                params!["task/parent", duplicate],
            )
            .unwrap();
        assert!(matches!(
            storage.pending_changes(),
            Err(StorageError::InvalidData(_))
        ));
    }

    #[test]
    fn regression_legacy_today_stabilizes_on_unrelated_edit() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let mut value = fixture();
        value["entries"]["task/parent"]["when"] = json!({"type":"Today","evening":false});
        let before = jiff::Zoned::now().date();
        storage.import_json(&value.to_string()).unwrap();
        let after = jiff::Zoned::now().date();
        let imported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let normalized = storage.load().unwrap().tasks["task/parent"].when.clone();
        let When::Scheduled { date } = normalized else {
            panic!("expected scheduled date")
        };
        assert!(date == before || date == after);
        value["entries"]["task/parent"]["when"] = serde_json::to_value(&normalized).unwrap();
        assert_eq!(imported, value);
        let initial_generation = generation(&storage);
        let initial_pending = storage.pending_changes().unwrap();
        assert_eq!(
            initial_pending
                .iter()
                .find(|change| change.key == "task/parent")
                .unwrap()
                .value
                .as_ref()
                .unwrap(),
            &imported["entries"]["task/parent"]
        );
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(generation(&storage), initial_generation);
        assert_eq!(storage.pending_changes().unwrap(), initial_pending);
        storage
            .transaction::<_, StorageError>(|store, _| {
                store.tasks.get_mut("task/parent").unwrap().title = "changed".into();
                Ok(())
            })
            .unwrap();
        let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        assert_eq!(
            exported["entries"]["task/parent"]["when"],
            serde_json::to_value(normalized).unwrap()
        );
        assert_eq!(generation(&storage), initial_generation + 1);
        let pending = storage.pending_changes().unwrap();
        assert_eq!(
            pending
                .iter()
                .find(|c| c.key == "task/parent")
                .unwrap()
                .value
                .as_ref()
                .unwrap()["when"],
            exported["entries"]["task/parent"]["when"]
        );
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(generation(&storage), initial_generation + 1);
        assert_eq!(storage.pending_changes().unwrap(), pending);
    }

    #[test]
    fn regression_legacy_anytime_stabilizes_on_noop() {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = SqliteStorage::open(dir.path().join("tasks.sqlite")).unwrap();
        let mut value = fixture();
        value["entries"]["task/parent"]["when"] = json!({"type":"Anytime"});
        storage.import_json(&value.to_string()).unwrap();
        let imported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        value["entries"]["task/parent"]["when"] = json!({"type":"Someday"});
        assert_eq!(imported, value);
        assert!(matches!(
            storage.load().unwrap().tasks["task/parent"].when,
            When::Someday
        ));
        let initial_generation = generation(&storage);
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        let exported: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        assert_eq!(
            exported["entries"]["task/parent"]["when"],
            json!({"type":"Someday"})
        );
        assert_eq!(generation(&storage), initial_generation);
        let pending = storage.pending_changes().unwrap();
        storage
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(generation(&storage), initial_generation);
        assert_eq!(storage.pending_changes().unwrap(), pending);
    }

    #[test]
    fn import_normalizes_all_legacy_schedules_once_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.sqlite");
        let mut source = fixture();
        source["entries"]["task/parent"]["when"] = json!({"type":"Today","evening":false});
        source["entries"]["task/child"]["when"] = json!({"type":"Today","evening":true});
        let mut storage = SqliteStorage::open(&path).unwrap();
        let before = jiff::Zoned::now().date();
        storage.import_json(&source.to_string()).unwrap();
        let after = jiff::Zoned::now().date();
        let stored: Value = serde_json::from_str(&storage.export_json().unwrap()).unwrap();
        let when = stored["entries"]["task/parent"]["when"].clone();
        assert_eq!(when["type"], "Scheduled");
        let date = when["date"]
            .as_str()
            .unwrap()
            .parse::<jiff::civil::Date>()
            .unwrap();
        assert!(date == before || date == after);
        assert_eq!(stored["entries"]["task/child"]["when"], when);
        source["entries"]["task/parent"]["when"] = when.clone();
        source["entries"]["task/child"]["when"] = when;
        assert_eq!(stored, source);
        let pending = storage.pending_changes().unwrap();
        let version = generation(&storage);
        drop(storage);
        let mut reopened = SqliteStorage::open(path).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&reopened.export_json().unwrap()).unwrap(),
            stored
        );
        assert!(reopened.load().unwrap().tasks.contains_key("task/parent"));
        reopened
            .transaction::<_, StorageError>(|_, _| Ok(()))
            .unwrap();
        assert_eq!(generation(&reopened), version);
        assert_eq!(reopened.pending_changes().unwrap(), pending);
    }
}
