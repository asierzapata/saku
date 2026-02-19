use opendal::Operator;
use opendal::services::S3;

use crate::config::StorageSection;

/// Build an opendal S3-compatible Operator from config.
pub fn build_operator(config: &StorageSection) -> anyhow::Result<Operator> {
    let builder = S3::default()
        .bucket(&config.bucket)
        .region(&config.region)
        .endpoint(&config.endpoint)
        .access_key_id(&config.access_key_id)
        .secret_access_key(&config.secret_access_key);

    let op = Operator::new(builder)?.finish();
    Ok(op)
}

/// Build the object key for a user's file.
/// Format: `{user_id}/{tool}/{path}`
pub fn object_key(user_id: &str, tool: &str, path: &str) -> String {
    format!("{}/{}/{}", user_id, tool, path)
}
