use std::collections::HashSet;

use saku_crypto::{MasterKey, decrypt_entry, encrypt_entry};
use serde_json::Value;

use crate::SyncError;

#[derive(Debug, Clone)]
pub struct PendingEntry {
    pub key: String,
    pub revision: i64,
    pub value: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub key: String,
    pub blob: Vec<u8>,
    pub deleted: bool,
}

pub struct PullPage {
    pub entries: Vec<RemoteEntry>,
    pub cookie: String,
    pub has_more: bool,
}

pub trait Replica {
    fn prepare_sync(&mut self, target: &str) -> Result<Option<String>, SyncError>;
    /// Requeue the complete local state and clear only the cursor atomically.
    fn reset_sync(&mut self, target: &str, expected_cookie: Option<&str>) -> Result<(), SyncError>;
    fn apply_pull(
        &mut self,
        target: &str,
        expected_cookie: Option<&str>,
        cookie: &str,
        entries: &[(String, String)],
    ) -> Result<(), SyncError>;
    fn pending(&self) -> Result<Vec<PendingEntry>, SyncError>;
    fn acknowledge(&mut self, changes: &[(String, i64)]) -> Result<(), SyncError>;
}

pub trait Transport {
    fn pull(&mut self, cookie: Option<&str>) -> Result<PullPage, SyncError>;
    fn push(&mut self, entries: &[(String, Vec<u8>)], base_cookie: &str) -> Result<Vec<String>, SyncError>;
}

#[derive(Debug)]
pub struct SyncOutcome {
    pub pulled: usize,
    pub pushed: usize,
}

pub fn sync_replica(
    replica: &mut impl Replica,
    transport: &mut impl Transport,
    master_key: &MasterKey,
    target: &str,
) -> Result<SyncOutcome, SyncError> {
    let mut cookie = replica.prepare_sync(target)?;
    let mut pulled = 0;
    let mut pushed = 0;
    'attempt: for attempt in 0..4 {
        let mut seen = HashSet::new();
        if let Some(cookie) = &cookie {
            seen.insert(cookie.clone());
        }
        loop {
            let page = match transport.pull(cookie.as_deref()) {
                Ok(page) => page,
                Err(SyncError::CursorInvalidated) => {
                    replica.reset_sync(target, cookie.as_deref())?;
                    cookie = None;
                    if attempt == 3 { return Err(SyncError::CursorInvalidated); }
                    continue 'attempt;
                }
                Err(error) => return Err(error),
            };
            if page.has_more && page.entries.is_empty() {
                return Err(SyncError::Backend {
                    message: "pull page has no entries but has_more is set".into(),
                });
            }
            if seen.contains(&page.cookie)
                && (page.has_more
                    || !page.entries.is_empty()
                    || cookie.as_deref() != Some(page.cookie.as_str()))
            {
                return Err(SyncError::Backend {
                    message: "pull cursor repeated".into(),
                });
            }
            seen.insert(page.cookie.clone());
            let mut entries = Vec::with_capacity(page.entries.len());
            for entry in page.entries {
                if entry.deleted {
                    return Err(SyncError::Backend {
                        message: "unsupported remote hard deletion".into(),
                    });
                }
                let bytes = decrypt_entry(&entry.blob, master_key)?;
                let text = String::from_utf8(bytes).map_err(|_| SyncError::Backend {
                    message: "remote entry is not valid UTF-8".into(),
                })?;
                entries.push((entry.key, text));
            }
            replica.apply_pull(target, cookie.as_deref(), &page.cookie, &entries)?;
            pulled += entries.len();
            cookie = Some(page.cookie);
            if !page.has_more {
                break;
            }
        }
        let pending = replica.pending()?;
        if pending.iter().any(|entry| entry.value.is_none()) {
            return Err(SyncError::Backend {
                message: "unsupported local hard deletion".into(),
            });
        }
        let base_cookie = cookie.as_deref().ok_or_else(|| SyncError::Backend {
            message: "missing completed pull cursor".into(),
        });
        let base_cookie = base_cookie?;
        let mut stale = false;
        for chunk in pending.chunks(50) {
            let batch = chunk
                .iter()
                .map(|entry| {
                    let plaintext = serde_json::to_vec(entry.value.as_ref().unwrap())?;
                    Ok((entry.key.clone(), encrypt_entry(&plaintext, master_key)))
                })
                .collect::<Result<Vec<_>, SyncError>>()?;
            let acknowledged = match transport.push(&batch, base_cookie) {
                Ok(keys) => keys,
                Err(SyncError::StaleWrite) => {
                    stale = true;
                    break;
                }
                Err(SyncError::CursorInvalidated) => {
                    replica.reset_sync(target, cookie.as_deref())?;
                    cookie = None;
                    if attempt == 3 { return Err(SyncError::CursorInvalidated); }
                    continue 'attempt;
                }
                Err(error) => return Err(error),
            };
            let requested: HashSet<_> = batch.iter().map(|(key, _)| key.as_str()).collect();
            let returned: HashSet<_> = acknowledged.iter().map(String::as_str).collect();
            if requested.len() != batch.len()
                || returned.len() != acknowledged.len()
                || returned != requested
            {
                return Err(SyncError::Backend {
                    message: "invalid push acknowledgement keys".into(),
                });
            }
            let revisions: Vec<_> = chunk
                .iter()
                .map(|entry| (entry.key.clone(), entry.revision))
                .collect();
            replica.acknowledge(&revisions)?;
            pushed += chunk.len();
        }
        if !stale {
            return Ok(SyncOutcome { pulled, pushed });
        }
    }
    Err(SyncError::StaleWrite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::OnceLock;

    fn key() -> &'static MasterKey {
        static KEY: OnceLock<MasterKey> = OnceLock::new();
        KEY.get_or_init(|| saku_crypto::kdf::derive_master_key(b"replica tests", &[1; 16]).unwrap())
    }

    #[derive(Default)]
    struct FakeReplica {
        cookie: Option<String>,
        entries: Vec<(String, String)>,
        pending: Vec<PendingEntry>,
        all: Vec<PendingEntry>,
        resets: Vec<Option<String>>,
        fail_reset: bool,
    }

    impl Replica for FakeReplica {
        fn prepare_sync(&mut self, _: &str) -> Result<Option<String>, SyncError> {
            Ok(self.cookie.clone())
        }
        fn reset_sync(&mut self, _: &str, expected: Option<&str>) -> Result<(), SyncError> {
            assert_eq!(self.cookie.as_deref(), expected);
            self.resets.push(expected.map(str::to_owned));
            if self.fail_reset {
                return Err(SyncError::Backend { message: "injected reset failure".into() });
            }
            self.pending = self.all.clone();
            self.cookie = None;
            Ok(())
        }
        fn apply_pull(
            &mut self,
            _: &str,
            expected: Option<&str>,
            cookie: &str,
            entries: &[(String, String)],
        ) -> Result<(), SyncError> {
            assert_eq!(self.cookie.as_deref(), expected);
            self.entries.extend_from_slice(entries);
            self.cookie = Some(cookie.into());
            Ok(())
        }
        fn pending(&self) -> Result<Vec<PendingEntry>, SyncError> {
            Ok(self.pending.clone())
        }
        fn acknowledge(&mut self, changes: &[(String, i64)]) -> Result<(), SyncError> {
            self.pending
                .retain(|e| !changes.contains(&(e.key.clone(), e.revision)));
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeTransport {
        pages: VecDeque<PullPage>,
        response: Option<Vec<String>>,
        push_count: usize,
        fail_on_push: Option<usize>,
        stale_on_push: Vec<usize>,
        base_cookies: Vec<String>,
        pushed_keys: Vec<Vec<String>>,
        invalid_pull: Vec<usize>,
        invalid_push: Vec<usize>,
        pull_cookies: Vec<Option<String>>,
    }

    impl Transport for FakeTransport {
        fn pull(&mut self, cookie: Option<&str>) -> Result<PullPage, SyncError> {
            self.pull_cookies.push(cookie.map(str::to_owned));
            if self.invalid_pull.contains(&self.pull_cookies.len()) {
                return Err(SyncError::CursorInvalidated);
            }
            self.pages.pop_front().ok_or_else(|| SyncError::Backend {
                message: "no page".into(),
            })
        }
        fn push(&mut self, entries: &[(String, Vec<u8>)], base_cookie: &str) -> Result<Vec<String>, SyncError> {
            self.push_count += 1;
            self.base_cookies.push(base_cookie.to_owned());
            self.pushed_keys.push(entries.iter().map(|(key, _)| key.clone()).collect());
            if self.invalid_push.contains(&self.push_count) {
                return Err(SyncError::CursorInvalidated);
            }
            if self.stale_on_push.contains(&self.push_count) {
                return Err(SyncError::StaleWrite);
            }
            if self.fail_on_push == Some(self.push_count) {
                return Err(SyncError::Backend {
                    message: "injected push failure".into(),
                });
            }
            Ok(self
                .response
                .clone()
                .unwrap_or_else(|| entries.iter().map(|(k, _)| k.clone()).collect()))
        }
    }

    fn page(cookie: &str, has_more: bool, entries: Vec<RemoteEntry>) -> PullPage {
        PullPage {
            entries,
            cookie: cookie.into(),
            has_more,
        }
    }

    #[test]
    fn replica_paginates_then_pushes_in_batches() {
        let mut replica = FakeReplica {
            pending: (0..51)
                .map(|n| PendingEntry {
                    key: format!("task/{n}"),
                    revision: n + 1,
                    value: Some(json!({"n":n})),
                })
                .collect(),
            ..Default::default()
        };
        let entry = RemoteEntry {
            key: "task/remote".into(),
            blob: encrypt_entry(b"{\"a\":true}", key()),
            deleted: false,
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", true, vec![entry]), page("2", false, vec![])]),
            ..Default::default()
        };
        let outcome = sync_replica(&mut replica, &mut transport, key(), "target").unwrap();
        assert_eq!((outcome.pulled, outcome.pushed), (1, 51));
        assert_eq!(transport.push_count, 2);
        assert_eq!(replica.cookie.as_deref(), Some("2"));
        assert_eq!(
            replica.entries,
            vec![("task/remote".into(), "{\"a\":true}".into())]
        );
        assert!(replica.pending.is_empty());
    }

    #[test]
    fn replica_rejects_incomplete_duplicate_or_unknown_acknowledgements() {
        for response in [
            vec![],
            vec!["task/a".into(), "task/a".into()],
            vec!["task/other".into()],
        ] {
            let mut replica = FakeReplica {
                pending: vec![PendingEntry {
                    key: "task/a".into(),
                    revision: 1,
                    value: Some(json!({})),
                }],
                ..Default::default()
            };
            let mut transport = FakeTransport {
                pages: VecDeque::from([page("1", false, vec![])]),
                response: Some(response),
                ..Default::default()
            };
            assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
            assert_eq!(replica.pending.len(), 1);
            assert_eq!(replica.cookie.as_deref(), Some("1"));
        }
    }

    #[test]
    fn replica_keeps_failed_batch_after_acknowledging_prior_batch() {
        let mut replica = FakeReplica {
            pending: (0..51)
                .map(|n| PendingEntry {
                    key: format!("task/{n}"),
                    revision: n + 1,
                    value: Some(json!({"n": n})),
                })
                .collect(),
            ..Default::default()
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![])]),
            fail_on_push: Some(2),
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
        assert_eq!(replica.pending.len(), 1);
        assert_eq!(replica.pending[0].key, "task/50");
        assert_eq!(replica.cookie.as_deref(), Some("1"));
        assert_eq!(transport.push_count, 2);
        assert_eq!(transport.base_cookies, ["1", "1"]);
    }

    #[test]
    fn stale_write_repulls_and_uses_new_completed_cookie() {
        let mut replica = FakeReplica {
            pending: vec![PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({"title":"local"})) }],
            ..Default::default()
        };
        let entry = RemoteEntry { key: "task/remote".into(), blob: encrypt_entry(b"{}", key()), deleted: false };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![]), page("2", false, vec![entry])]),
            stale_on_push: vec![1],
            ..Default::default()
        };
        let outcome = sync_replica(&mut replica, &mut transport, key(), "target").unwrap();
        assert_eq!((outcome.pulled, outcome.pushed), (1, 1));
        assert_eq!(transport.base_cookies, ["1", "2"]);
        assert_eq!(replica.cookie.as_deref(), Some("2"));
        assert!(replica.pending.is_empty());
    }

    #[test]
    fn stale_retries_are_bounded_and_keep_pending() {
        let mut replica = FakeReplica {
            pending: vec![PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) }],
            ..Default::default()
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![]), page("1", false, vec![]), page("1", false, vec![]), page("1", false, vec![])]),
            stale_on_push: vec![1, 2, 3, 4],
            ..Default::default()
        };
        assert!(matches!(sync_replica(&mut replica, &mut transport, key(), "target"), Err(SyncError::StaleWrite)));
        assert_eq!(transport.push_count, 4);
        assert_eq!(replica.pending.len(), 1);
    }

    #[test]
    fn generic_push_failure_is_not_retried() {
        let mut replica = FakeReplica {
            pending: vec![PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) }],
            ..Default::default()
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![])]),
            fail_on_push: Some(1),
            ..Default::default()
        };
        assert!(matches!(sync_replica(&mut replica, &mut transport, key(), "target"), Err(SyncError::Backend { .. })));
        assert_eq!(transport.push_count, 1);
        assert_eq!(replica.pending.len(), 1);
    }

    #[test]
    fn stale_after_acknowledged_chunk_resnapshots_pending() {
        let mut replica = FakeReplica {
            pending: (0..51).map(|n| PendingEntry { key: format!("task/{n}"), revision: n + 1, value: Some(json!({"n":n})) }).collect(),
            ..Default::default()
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![]), page("2", false, vec![])]),
            stale_on_push: vec![2],
            ..Default::default()
        };
        let outcome = sync_replica(&mut replica, &mut transport, key(), "target").unwrap();
        assert_eq!(outcome.pushed, 51);
        assert_eq!(transport.base_cookies, ["1", "1", "2"]);
        assert_eq!(transport.pushed_keys[0].len(), 50);
        assert_eq!(transport.pushed_keys[1], ["task/50"]);
        assert_eq!(transport.pushed_keys[2], ["task/50"]);
        assert!(replica.pending.is_empty());
    }

    #[test]
    fn undecryptable_retry_page_does_not_advance_or_push_again() {
        let mut replica = FakeReplica {
            pending: vec![PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) }],
            ..Default::default()
        };
        let invalid = RemoteEntry { key: "task/remote".into(), blob: vec![0], deleted: false };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", false, vec![]), page("2", false, vec![invalid])]),
            stale_on_push: vec![1],
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
        assert_eq!(replica.cookie.as_deref(), Some("1"));
        assert_eq!(replica.pending.len(), 1);
        assert_eq!(transport.push_count, 1);
    }

    #[test]
    fn invalidated_partial_pull_restarts_from_none_before_pushing() {
        let entry = RemoteEntry { key: "task/remote".into(), blob: encrypt_entry(b"{}", key()), deleted: false };
        let local = PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) };
        let mut replica = FakeReplica { pending: vec![local.clone()], all: vec![local], ..Default::default() };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("old1", true, vec![entry]), page("new0", false, vec![])]),
            invalid_pull: vec![2],
            ..Default::default()
        };
        let result = sync_replica(&mut replica, &mut transport, key(), "target").unwrap();
        assert_eq!((result.pulled, result.pushed), (1, 1));
        assert_eq!(transport.pull_cookies, [None, Some("old1".into()), None]);
        assert_eq!(replica.resets, [Some("old1".into())]);
        assert_eq!(transport.base_cookies, ["new0"]);
        assert!(replica.pending.is_empty());
    }

    #[test]
    fn invalidation_after_acknowledged_chunk_requeues_all_values() {
        let all: Vec<_> = (0..51).map(|n| PendingEntry {
            key: format!("task/{n}"), revision: n + 1, value: Some(json!({"n":n}))
        }).collect();
        let mut replica = FakeReplica { pending: all.clone(), all, ..Default::default() };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("old", false, vec![]), page("new", false, vec![])]),
            invalid_push: vec![2],
            ..Default::default()
        };
        let outcome = sync_replica(&mut replica, &mut transport, key(), "target").unwrap();
        assert_eq!(outcome.pushed, 101);
        assert_eq!(replica.resets, [Some("old".into())]);
        assert_eq!(transport.pull_cookies, [None, None]);
        assert_eq!(transport.base_cookies, ["old", "old", "new", "new"]);
        assert_eq!(transport.pushed_keys.iter().map(Vec::len).collect::<Vec<_>>(), [50, 1, 50, 1]);
        assert!(replica.pending.is_empty());
    }

    #[test]
    fn invalidation_retries_are_bounded_and_reset_failure_aborts() {
        let local = PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) };
        let mut replica = FakeReplica { pending: vec![local.clone()], all: vec![local.clone()], ..Default::default() };
        let mut transport = FakeTransport { invalid_pull: vec![1, 2, 3, 4], ..Default::default() };
        assert!(matches!(sync_replica(&mut replica, &mut transport, key(), "target"), Err(SyncError::CursorInvalidated)));
        assert_eq!(transport.pull_cookies, [None, None, None, None]);
        assert_eq!(replica.resets, [None, None, None, None]);
        assert_eq!(replica.pending.len(), 1);
        assert_eq!(transport.push_count, 0);
        let mut replica = FakeReplica { cookie: Some("old".into()), pending: vec![local.clone()], all: vec![local], fail_reset: true, ..Default::default() };
        let mut transport = FakeTransport { invalid_pull: vec![1], ..Default::default() };
        assert!(matches!(sync_replica(&mut replica, &mut transport, key(), "target"), Err(SyncError::Backend { .. })));
        assert_eq!(replica.cookie.as_deref(), Some("old"));
        assert_eq!(transport.pull_cookies, [Some("old".into())]);
        assert_eq!(transport.push_count, 0);
    }

    #[test]
    fn invalid_ciphertext_on_recovery_keeps_requeued_values() {
        let local = PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) };
        let mut replica = FakeReplica { cookie: Some("old".into()), all: vec![local], ..Default::default() };
        let bad = RemoteEntry { key: "task/remote".into(), blob: vec![0], deleted: false };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("new", false, vec![bad])]),
            invalid_pull: vec![1],
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
        assert_eq!(replica.resets, [Some("old".into())]);
        assert_eq!(replica.cookie, None);
        assert_eq!(replica.pending.len(), 1);
        assert_eq!(transport.push_count, 0);
    }

    #[test]
    fn stale_and_invalidation_share_the_same_attempt_budget() {
        let local = PendingEntry { key: "task/a".into(), revision: 1, value: Some(json!({})) };
        let mut replica = FakeReplica { pending: vec![local.clone()], all: vec![local], ..Default::default() };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("one", false, vec![]), page("two", false, vec![]), page("three", false, vec![]), page("three", false, vec![])]),
            stale_on_push: vec![1, 3, 4],
            invalid_push: vec![2],
            ..Default::default()
        };
        assert!(matches!(sync_replica(&mut replica, &mut transport, key(), "target"), Err(SyncError::StaleWrite)));
        assert_eq!(transport.push_count, 4);
        assert_eq!(replica.resets, [Some("two".into())]);
        assert_eq!(transport.pull_cookies, [None, Some("one".into()), None, Some("three".into())]);
        assert_eq!(replica.pending.len(), 1);
    }

    #[test]
    fn replica_rejects_hard_deletes_invalid_utf8_and_stalled_cursor() {
        let cases = [
            RemoteEntry {
                key: "task/a".into(),
                blob: vec![],
                deleted: true,
            },
            RemoteEntry {
                key: "task/a".into(),
                blob: encrypt_entry(&[0xff], key()),
                deleted: false,
            },
            RemoteEntry {
                key: "task/a".into(),
                blob: vec![0],
                deleted: false,
            },
        ];
        for entry in cases {
            let mut replica = FakeReplica::default();
            let mut transport = FakeTransport {
                pages: VecDeque::from([page("1", false, vec![entry])]),
                ..Default::default()
            };
            assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
            assert_eq!(replica.cookie, None);
        }
        let mut replica = FakeReplica {
            cookie: Some("1".into()),
            ..Default::default()
        };
        let mut transport = FakeTransport {
            pages: VecDeque::from([page("1", true, vec![])]),
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut transport, key(), "target").is_err());
        assert_eq!(replica.cookie.as_deref(), Some("1"));
    }

    #[test]
    fn replica_rejects_empty_paginated_pages_and_longer_cursor_cycles() {
        let mut replica = FakeReplica::default();
        let mut empty = FakeTransport {
            pages: VecDeque::from([page("1", true, vec![])]),
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut empty, key(), "target").is_err());
        assert_eq!(replica.cookie, None);
        let entry = || RemoteEntry {
            key: "task/a".into(),
            blob: encrypt_entry(b"{}", key()),
            deleted: false,
        };
        let mut cycle = FakeTransport {
            pages: VecDeque::from([
                page("a", true, vec![entry()]),
                page("b", true, vec![entry()]),
                page("a", true, vec![entry()]),
            ]),
            ..Default::default()
        };
        assert!(sync_replica(&mut replica, &mut cycle, key(), "target").is_err());
        assert_eq!(replica.cookie.as_deref(), Some("b"));
        assert_eq!(replica.entries.len(), 2);
    }
}
