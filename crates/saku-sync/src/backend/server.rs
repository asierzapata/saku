use crate::backend::SyncBackend;
use crate::error::SyncError;

#[cfg(not(feature = "server"))]
pub struct ServerSyncBackend;

#[cfg(not(feature = "server"))]
impl SyncBackend for ServerSyncBackend {
    fn fetch(&self, _tool: &str, _path: &str) -> Result<Vec<u8>, SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend requires the 'server' feature".to_string(),
        })
    }

    fn push(&self, _tool: &str, _path: &str, _data: &[u8]) -> Result<(), SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend requires the 'server' feature".to_string(),
        })
    }

    fn fetch_merkle(&self) -> Result<Option<Vec<u8>>, SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend requires the 'server' feature".to_string(),
        })
    }

    fn push_merkle(&self, _data: &[u8]) -> Result<(), SyncError> {
        Err(SyncError::Backend {
            message: "ServerSyncBackend requires the 'server' feature".to_string(),
        })
    }

    fn is_reachable(&self) -> bool {
        false
    }
}

#[cfg(feature = "server")]
mod impl_server {
    use std::cell::RefCell;

    use super::*;
    use crate::backend::server_types::*;

    const MERKLE_TOOL: &str = "_saku";
    const MERKLE_PATH: &str = "merkle.json";

    pub struct ServerSyncBackend {
        server_url: String,
        access_token: RefCell<String>,
        refresh_token: RefCell<String>,
        #[allow(dead_code)]
        device_id: String,
        http_client: ureq::Agent,
    }

    impl ServerSyncBackend {
        /// Create a new ServerSyncBackend, reading tokens from the OS keychain.
        pub fn new(server_url: &str, device_id: &str) -> Result<Self, SyncError> {
            let creds = saku_crypto::keychain::SyncCredentialStore::new()
                .and_then(|s| s.load_or_migrate())
                .unwrap_or_default();

            let access_token = creds.access_token.unwrap_or_default();
            let refresh_token = creds.refresh_token.unwrap_or_default();

            let http_client = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(5))
                .timeout_read(std::time::Duration::from_secs(30))
                .build();

            Ok(Self {
                server_url: server_url.trim_end_matches('/').to_string(),
                access_token: RefCell::new(access_token),
                refresh_token: RefCell::new(refresh_token),
                device_id: device_id.to_string(),
                http_client,
            })
        }

        /// Execute a closure with automatic auth retry on 401.
        fn with_auth_retry<F, T>(&self, f: F) -> Result<T, SyncError>
        where
            F: Fn(&str) -> Result<T, SyncError>,
        {
            let token = self.access_token.borrow().clone();
            match f(&token) {
                Ok(val) => Ok(val),
                Err(SyncError::Backend { ref message }) if message.contains("401") => {
                    // Try refreshing the token
                    self.refresh_access_token()?;
                    let new_token = self.access_token.borrow().clone();
                    f(&new_token)
                }
                Err(e) => Err(e),
            }
        }

        /// Refresh the access token using the refresh token.
        fn refresh_access_token(&self) -> Result<(), SyncError> {
            let refresh = self.refresh_token.borrow().clone();
            if refresh.is_empty() {
                return Err(SyncError::Backend {
                    message: "No refresh token available".to_string(),
                });
            }

            let url = format!("{}/api/v1/auth/refresh", self.server_url);
            let req_body = RefreshRequest {
                refresh_token: refresh,
            };

            let resp = self
                .http_client
                .post(&url)
                .send_json(ureq::json!({
                    "refresh_token": req_body.refresh_token,
                }))
                .map_err(|e| SyncError::Backend {
                    message: format!("Refresh token request failed: {e}"),
                })?;

            let body: RefreshResponse = resp.into_json().map_err(|e| SyncError::Backend {
                message: format!("Failed to parse refresh response: {e}"),
            })?;

            // Update in-memory tokens
            *self.access_token.borrow_mut() = body.access_token.clone();
            *self.refresh_token.borrow_mut() = body.refresh_token.clone();

            // Persist to keychain (read-modify-write)
            if let Ok(store) = saku_crypto::keychain::SyncCredentialStore::new() {
                let mut creds = store.load().unwrap_or_default();
                creds.access_token = Some(body.access_token.clone());
                creds.refresh_token = Some(body.refresh_token.clone());
                let _ = store.store(&creds);
            }

            Ok(())
        }

        /// GET a presigned download URL from the server, then GET the actual bytes.
        fn fetch_via_presign(&self, tool: &str, path: &str) -> Result<Vec<u8>, SyncError> {
            self.with_auth_retry(|token| {
                let url = format!(
                    "{}/api/v1/sync/{}/download-url?path={}",
                    self.server_url, tool, path
                );

                let resp = self
                    .http_client
                    .get(&url)
                    .set("Authorization", &format!("Bearer {}", token))
                    .call()
                    .map_err(|e| SyncError::Backend {
                        message: format!("{e}"),
                    })?;

                let presigned: PresignedUrlResponse =
                    resp.into_json().map_err(|e| SyncError::Backend {
                        message: format!("Failed to parse presigned URL response: {e}"),
                    })?;

                // Download from presigned URL
                let data_resp = self.http_client.get(&presigned.url).call().map_err(|e| {
                    SyncError::Backend {
                        message: format!("Download from presigned URL failed: {e}"),
                    }
                })?;

                let mut bytes = Vec::new();
                data_resp
                    .into_reader()
                    .read_to_end(&mut bytes)
                    .map_err(SyncError::Io)?;

                Ok(bytes)
            })
        }

        /// POST to get a presigned upload URL, then PUT the data.
        fn push_via_presign(&self, tool: &str, path: &str, data: &[u8]) -> Result<(), SyncError> {
            let data_vec = data.to_vec();

            self.with_auth_retry(|token| {
                // Get presigned upload URL
                let url = format!("{}/api/v1/sync/{}/upload-url", self.server_url, tool);

                let resp = self
                    .http_client
                    .post(&url)
                    .set("Authorization", &format!("Bearer {}", token))
                    .send_json(ureq::json!({
                        "path": path,
                        "content_length": data_vec.len() as i64,
                    }))
                    .map_err(|e| SyncError::Backend {
                        message: format!("{e}"),
                    })?;

                let upload_resp: UploadUrlResponse =
                    resp.into_json().map_err(|e| SyncError::Backend {
                        message: format!("Failed to parse upload URL response: {e}"),
                    })?;

                // PUT data to presigned URL
                self.http_client
                    .put(&upload_resp.url)
                    .set("Content-Type", "application/octet-stream")
                    .send_bytes(&data_vec)
                    .map_err(|e| SyncError::Backend {
                        message: format!("Upload to presigned URL failed: {e}"),
                    })?;

                // Confirm upload
                let confirm_url =
                    format!("{}/api/v1/sync/{}/confirm-upload", self.server_url, tool);
                self.http_client
                    .post(&confirm_url)
                    .set("Authorization", &format!("Bearer {}", token))
                    .send_json(ureq::json!({
                        "path": path,
                        "size_bytes": data_vec.len() as i64,
                    }))
                    .map_err(|e| SyncError::Backend {
                        message: format!("Confirm upload failed: {e}"),
                    })?;

                Ok(())
            })
        }
    }

    impl SyncBackend for ServerSyncBackend {
        fn fetch(&self, tool: &str, path: &str) -> Result<Vec<u8>, SyncError> {
            self.fetch_via_presign(tool, path)
        }

        fn push(&self, tool: &str, path: &str, data: &[u8]) -> Result<(), SyncError> {
            self.push_via_presign(tool, path, data)
        }

        fn fetch_merkle(&self) -> Result<Option<Vec<u8>>, SyncError> {
            match self.fetch_via_presign(MERKLE_TOOL, MERKLE_PATH) {
                Ok(data) => Ok(Some(data)),
                Err(SyncError::Backend { ref message }) if message.contains("404") => Ok(None),
                Err(e) => Err(e),
            }
        }

        fn push_merkle(&self, data: &[u8]) -> Result<(), SyncError> {
            self.push_via_presign(MERKLE_TOOL, MERKLE_PATH, data)
        }

        fn is_reachable(&self) -> bool {
            let url = format!("{}/api/v1/health", self.server_url);
            self.http_client
                .head(&url)
                .timeout(std::time::Duration::from_secs(2))
                .call()
                .is_ok()
        }
    }
}

#[cfg(feature = "server")]
pub use impl_server::ServerSyncBackend;
