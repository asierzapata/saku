use std::cell::RefCell;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::replica::{PullPage, RemoteEntry, Replica, SyncOutcome, Transport, sync_replica};
use crate::{SyncError, hash::sha256_bytes};
use saku_crypto::kdf::{derive_deterministic_salt, derive_master_key};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for a KV sync run.
pub struct KvSyncConfig {
    pub server_url: String,
    pub tool: String,
    pub passphrase: Vec<u8>,
}

// ---------------------------------------------------------------------------
// HTTP response/request types (match server API)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GetEntriesResponse {
    entries: Vec<KvEntryResponse>,
    cookie: String,
    has_more: bool,
}

#[derive(Deserialize)]
struct KvEntryResponse {
    key: String,
    blob: String,
    seq: i64,
    deleted: bool,
}

#[derive(Serialize)]
struct BatchPutRequest {
    entries: Vec<BatchPutEntry>,
}

#[derive(Serialize, Clone)]
struct BatchPutEntry {
    key: String,
    blob: String,
}

#[derive(Deserialize)]
struct BatchPutResponse {
    results: Vec<BatchPutResult>,
    #[allow(dead_code)]
    cookie: String,
}

#[derive(Deserialize)]
struct BatchPutResult {
    key: String,
    #[allow(dead_code)]
    seq: i64,
}

// ---------------------------------------------------------------------------
// Auth refresh types (same as backend/server.rs)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

// ---------------------------------------------------------------------------
// HTTP client with auth retry
// ---------------------------------------------------------------------------

fn subject(token: &str) -> Result<String, SyncError> {
    let mut parts = token.split('.');
    let _header = parts.next();
    let payload = parts.next().ok_or_else(|| SyncError::Backend {
        message: "access token has no subject; log in again".into(),
    })?;
    if parts.next().is_none() || parts.next().is_some() {
        return Err(SyncError::Backend {
            message: "invalid access token; log in again".into(),
        });
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| SyncError::Backend {
            message: "invalid access token; log in again".into(),
        })?;
    let claims: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| SyncError::Backend {
            message: "invalid access token; log in again".into(),
        })?;
    claims["sub"]
        .as_str()
        .filter(|sub| !sub.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| SyncError::Backend {
            message: "access token has no subject; log in again".into(),
        })
}

fn require_subject(token: &str, expected: &str) -> Result<(), SyncError> {
    if subject(token)? != expected {
        return Err(SyncError::Backend {
            message: "refresh changed account; run 'tdo sync login'".into(),
        });
    }
    Ok(())
}

fn sync_target(
    server_url: &str,
    tool: &str,
    subject: &str,
    key: &[u8],
) -> Result<String, SyncError> {
    let scope = serde_json::to_vec(&(
        server_url.trim_end_matches('/'),
        tool,
        subject,
        sha256_bytes(key),
    ))?;
    Ok(sha256_bytes(&scope))
}

fn decode_pull_page(
    response: GetEntriesResponse,
    requested: Option<&str>,
) -> Result<PullPage, SyncError> {
    let parse_cursor = |text: &str| -> Result<i64, SyncError> {
        text.parse::<i64>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or_else(|| SyncError::Backend {
                message: "invalid pull cursor".into(),
            })
    };
    let mut previous = match requested {
        Some(cookie) => parse_cursor(cookie)?,
        None => 0,
    };
    for entry in &response.entries {
        if entry.seq <= previous {
            return Err(SyncError::Backend {
                message: "pull entry sequence did not advance".into(),
            });
        }
        previous = entry.seq;
    }
    if parse_cursor(&response.cookie)? != previous {
        return Err(SyncError::Backend {
            message: "pull response cursor does not match entries".into(),
        });
    }
    if response.has_more && response.entries.is_empty() {
        return Err(SyncError::Backend {
            message: "pull page has no entries but has_more is set".into(),
        });
    }
    let entries = response
        .entries
        .into_iter()
        .map(|entry| {
            let blob = STANDARD
                .decode(entry.blob)
                .map_err(|_| SyncError::Backend {
                    message: "invalid base64 in pull response".into(),
                })?;
            Ok(RemoteEntry {
                key: entry.key,
                blob,
                deleted: entry.deleted,
            })
        })
        .collect::<Result<Vec<_>, SyncError>>()?;
    Ok(PullPage {
        entries,
        cookie: response.cookie,
        has_more: response.has_more,
    })
}

struct KvHttpClient {
    server_url: String,
    access_token: RefCell<String>,
    refresh_token: RefCell<String>,
    subject: String,
    http: ureq::Agent,
}

impl KvHttpClient {
    fn new(server_url: &str) -> Result<Self, SyncError> {
        let creds = saku_crypto::keychain::SyncCredentialStore::new()
            .and_then(|s| s.load_or_migrate())
            .map_err(|_| SyncError::Backend {
                message: "sync credentials unavailable; run 'tdo sync login'".into(),
            })?;
        let token = creds
            .access_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| SyncError::Backend {
                message: "access token missing; run 'tdo sync login'".into(),
            })?;
        let subject = subject(&token)?;
        Ok(Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            access_token: RefCell::new(token),
            refresh_token: RefCell::new(creds.refresh_token.unwrap_or_default()),
            subject,
            http: ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(5))
                .timeout_read(std::time::Duration::from_secs(30))
                .build(),
        })
    }

    fn with_auth_retry<F, T>(&self, f: F) -> Result<T, SyncError>
    where
        F: Fn(&str) -> Result<T, SyncError>,
    {
        let token = self.access_token.borrow().clone();
        match f(&token) {
            Ok(value) => Ok(value),
            Err(SyncError::Backend { ref message }) if message.contains("401") => {
                self.refresh_access_token()?;
                f(&self.access_token.borrow())
            }
            Err(error) => Err(error),
        }
    }

    fn refresh_access_token(&self) -> Result<(), SyncError> {
        let refresh = self.refresh_token.borrow().clone();
        if refresh.is_empty() {
            return Err(SyncError::Backend {
                message: "refresh token missing; run 'tdo sync login'".into(),
            });
        }
        let response = self
            .http
            .post(&format!("{}/api/v1/auth/refresh", self.server_url))
            .send_json(ureq::json!({"refresh_token": refresh}))
            .map_err(|e| SyncError::Backend {
                message: format!("refresh request failed: {e}"),
            })?;
        let body: RefreshResponse = response.into_json().map_err(|e| SyncError::Backend {
            message: format!("invalid refresh response: {e}"),
        })?;
        require_subject(&body.access_token, &self.subject)?;
        *self.access_token.borrow_mut() = body.access_token.clone();
        *self.refresh_token.borrow_mut() = body.refresh_token.clone();
        // Persist to keychain
        if let Ok(store) = saku_crypto::keychain::SyncCredentialStore::new() {
            let mut creds = store.load().unwrap_or_default();
            creds.access_token = Some(body.access_token);
            creds.refresh_token = Some(body.refresh_token);
            let _ = store.store(&creds);
        }
        Ok(())
    }
}

struct HttpTransport<'a> {
    client: &'a KvHttpClient,
    tool: &'a str,
}

impl Transport for HttpTransport<'_> {
    /// GET /kv/{tool}?cookie={cookie}&limit={limit}
    fn pull(&mut self, cookie: Option<&str>) -> Result<PullPage, SyncError> {
        let response = self.client.with_auth_retry(|token| {
            let url = format!("{}/api/v1/kv/{}", self.client.server_url, self.tool);
            let mut request = self.client.http.get(&url).query("limit", "100");
            if let Some(cookie) = cookie {
                request = request.query("cookie", cookie);
            }
            request
                .set("Authorization", &format!("Bearer {token}"))
                .call()
                .map_err(|e| SyncError::Backend {
                    message: format!("{e}"),
                })?
                .into_json::<GetEntriesResponse>()
                .map_err(|e| SyncError::Backend {
                    message: format!("invalid pull response: {e}"),
                })
        })?;
        decode_pull_page(response, cookie)
    }

    /// PUT /kv/{tool} with batch entries
    fn push(&mut self, entries: &[(String, Vec<u8>)]) -> Result<Vec<String>, SyncError> {
        let response = self.client.with_auth_retry(|token| {
            let url = format!("{}/api/v1/kv/{}", self.client.server_url, self.tool);
            let body = BatchPutRequest {
                entries: entries
                    .iter()
                    .map(|(key, blob)| BatchPutEntry {
                        key: key.clone(),
                        blob: STANDARD.encode(blob),
                    })
                    .collect(),
            };
            self.client
                .http
                .put(&url)
                .set("Authorization", &format!("Bearer {token}"))
                .send_json(&body)
                .map_err(|e| SyncError::Backend {
                    message: format!("{e}"),
                })?
                .into_json::<BatchPutResponse>()
                .map_err(|e| SyncError::Backend {
                    message: format!("invalid push response: {e}"),
                })
        })?;
        Ok(response
            .results
            .into_iter()
            .map(|result| result.key)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Core sync logic
// ---------------------------------------------------------------------------

/// Run a full KV sync cycle: pull remote changes, merge, push dirty entries.
pub fn sync_kv(
    config: &KvSyncConfig,
    replica: &mut impl Replica,
) -> Result<SyncOutcome, SyncError> {
    let client = KvHttpClient::new(&config.server_url)?;
    // Derive master key
    let salt = derive_deterministic_salt(&config.passphrase);
    let key = derive_master_key(&config.passphrase, &salt)?;
    let target = sync_target(
        &client.server_url,
        &config.tool,
        &client.subject,
        key.as_bytes(),
    )?;
    let mut transport = HttpTransport {
        client: &client,
        tool: &config.tool,
    };
    sync_replica(replica, &mut transport, &key, &target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token(claims: serde_json::Value) -> String {
        format!(
            "header.{}.signature",
            URL_SAFE_NO_PAD.encode(claims.to_string())
        )
    }

    #[test]
    fn scope_binds_server_tool_subject_and_key_without_ambiguous_delimiters() {
        let original = sync_target("https://example.test/", "tdo", "alice", &[1; 32]).unwrap();
        assert_eq!(
            original,
            sync_target("https://example.test", "tdo", "alice", &[1; 32]).unwrap()
        );
        for alternate in [
            sync_target("https://other.test", "tdo", "alice", &[1; 32]).unwrap(),
            sync_target("https://example.test", "other", "alice", &[1; 32]).unwrap(),
            sync_target("https://example.test", "tdo", "bob", &[1; 32]).unwrap(),
            sync_target("https://example.test", "tdo", "alice", &[2; 32]).unwrap(),
        ] {
            assert_ne!(original, alternate);
        }
        assert_ne!(
            sync_target("a/b", "c", "d", &[1; 32]).unwrap(),
            sync_target("a", "b/c", "d", &[1; 32]).unwrap()
        );
    }

    #[test]
    fn refresh_subject_must_exist_and_match() {
        assert!(require_subject(&token(json!({"sub":"alice"})), "alice").is_ok());
        assert!(require_subject(&token(json!({"sub":"bob"})), "alice").is_err());
        assert!(subject(&token(json!({"sub":""}))).is_err());
        assert!(subject(&token(json!({}))).is_err());
        assert!(subject("not-a-jwt").is_err());
    }

    #[test]
    fn invalid_base64_aborts_entire_pull_page() {
        let response = GetEntriesResponse {
            entries: vec![
                KvEntryResponse {
                    key: "task/a".into(),
                    blob: STANDARD.encode(b"valid"),
                    seq: 1,
                    deleted: false,
                },
                KvEntryResponse {
                    key: "task/b".into(),
                    blob: "not base64!".into(),
                    seq: 2,
                    deleted: false,
                },
            ],
            cookie: "2".into(),
            has_more: false,
        };
        assert!(decode_pull_page(response, None).is_err());
        let deleted = GetEntriesResponse {
            entries: vec![KvEntryResponse {
                key: "task/a".into(),
                blob: "invalid base64!".into(),
                seq: 1,
                deleted: true,
            }],
            cookie: "1".into(),
            has_more: false,
        };
        assert!(decode_pull_page(deleted, None).is_err());
    }

    #[test]
    fn pull_sequences_and_cookies_must_be_consistent() {
        let response = |seqs: &[i64], cookie: &str, has_more: bool| GetEntriesResponse {
            entries: seqs
                .iter()
                .map(|seq| KvEntryResponse {
                    key: format!("task/{seq}"),
                    blob: STANDARD.encode(b"payload"),
                    seq: *seq,
                    deleted: false,
                })
                .collect(),
            cookie: cookie.into(),
            has_more,
        };
        for (seqs, cookie, more) in [
            (vec![6, 6], "6", false),
            (vec![7, 6], "6", false),
            (vec![5], "5", false),
            (vec![6], "5", false),
            (vec![], "6", false),
            (vec![], "5", true),
            (vec![6], "-1", false),
            (vec![6], "bad", false),
        ] {
            assert!(decode_pull_page(response(&seqs, cookie, more), Some("5")).is_err());
        }
        assert!(decode_pull_page(response(&[], "0", false), Some("-1")).is_err());
        assert!(decode_pull_page(response(&[], "0", false), Some("bad")).is_err());
        assert!(decode_pull_page(response(&[0], "0", false), None).is_err());
        assert!(decode_pull_page(response(&[6, 8], "8", false), Some("5")).is_ok());
        assert!(decode_pull_page(response(&[], "5", false), Some("5")).is_ok());
    }

    #[test]
    fn http_adapter_uses_pull_cookie_and_ignores_push_cookie() {
        use crate::replica::PendingEntry;
        use std::io::{Read, Write};
        use std::net::TcpListener;

        struct FakeReplica {
            cookie: Option<String>,
            pending: Vec<PendingEntry>,
            applied: Vec<(String, String)>,
        }
        impl Replica for FakeReplica {
            fn prepare_sync(&mut self, _: &str) -> Result<Option<String>, SyncError> {
                Ok(self.cookie.clone())
            }
            fn apply_pull(
                &mut self,
                _: &str,
                expected: Option<&str>,
                cookie: &str,
                entries: &[(String, String)],
            ) -> Result<(), SyncError> {
                assert_eq!(self.cookie.as_deref(), expected);
                self.applied.extend_from_slice(entries);
                self.cookie = Some(cookie.into());
                Ok(())
            }
            fn pending(&self) -> Result<Vec<PendingEntry>, SyncError> {
                Ok(self.pending.clone())
            }
            fn acknowledge(&mut self, changes: &[(String, i64)]) -> Result<(), SyncError> {
                self.pending
                    .retain(|entry| !changes.contains(&(entry.key.clone(), entry.revision)));
                Ok(())
            }
        }

        let master = derive_master_key(b"loopback only", &[9; 16]).unwrap();
        let first = STANDARD.encode(saku_crypto::encrypt_entry(br#"{"title":"one"}"#, &master));
        let second = STANDARD.encode(saku_crypto::encrypt_entry(br#"{"title":"two"}"#, &master));
        let responses = [
            json!({"entries":[{"key":"task/remote1","blob":first,"seq":8,"deleted":false}],"cookie":"8","has_more":false}),
            json!({"results":[{"key":"task/local","seq":99}],"cookie":"99999"}),
            json!({"entries":[{"key":"task/remote2","blob":second,"seq":9,"deleted":false}],"cookie":"9","has_more":false}),
        ];
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut data = Vec::new();
                let end = loop {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    data.extend_from_slice(&chunk[..count]);
                    if let Some(offset) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        break offset + 4;
                    }
                };
                let headers = String::from_utf8(data[..end].to_vec()).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                while data.len() < end + length {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    data.extend_from_slice(&chunk[..count]);
                }
                requests.push((
                    headers.lines().next().unwrap().to_owned(),
                    data[end..end + length].to_vec(),
                ));
                let body = response.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
            requests
        });
        let client = KvHttpClient {
            server_url: format!("http://{address}"),
            access_token: RefCell::new("test-token".into()),
            refresh_token: RefCell::new(String::new()),
            subject: "test-subject".into(),
            http: ureq::AgentBuilder::new()
                .timeout_read(std::time::Duration::from_secs(5))
                .build(),
        };
        let mut transport = HttpTransport {
            client: &client,
            tool: "tdo",
        };
        let mut replica = FakeReplica {
            cookie: Some("7".into()),
            pending: vec![PendingEntry {
                key: "task/local".into(),
                revision: 4,
                value: Some(json!({"title":"local"})),
            }],
            applied: vec![],
        };
        sync_replica(&mut replica, &mut transport, &master, "target").unwrap();
        assert_eq!(replica.cookie.as_deref(), Some("8"));
        assert!(replica.pending.is_empty());
        sync_replica(&mut replica, &mut transport, &master, "target").unwrap();
        assert_eq!(replica.cookie.as_deref(), Some("9"));
        assert_eq!(replica.applied.len(), 2);
        let requests = server.join().unwrap();
        assert!(requests[0].0.starts_with("GET /api/v1/kv/tdo?"));
        assert!(requests[0].0.contains("limit=100"));
        assert!(requests[0].0.contains("cookie=7"));
        assert!(requests[1].0.starts_with("PUT /api/v1/kv/tdo "));
        let body: serde_json::Value = serde_json::from_slice(&requests[1].1).unwrap();
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        assert_eq!(body["entries"][0]["key"], "task/local");
        let blob = STANDARD
            .decode(body["entries"][0]["blob"].as_str().unwrap())
            .unwrap();
        let plaintext = saku_crypto::decrypt_entry(&blob, &master).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&plaintext).unwrap(),
            json!({"title":"local"})
        );
        assert!(requests[2].0.contains("cookie=8"));
        assert!(!requests[2].0.contains("99999"));
    }
}
