use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// User ID (UUID)
    pub sub: String,
    /// Device ID
    pub device_id: String,
    /// Expiration (Unix timestamp)
    pub exp: u64,
    /// Issued at (Unix timestamp)
    pub iat: u64,
}

/// Encode an access token.
pub fn encode_access_token(
    user_id: &str,
    device_id: &str,
    secret: &str,
    ttl_mins: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = jiff::Timestamp::now().as_second() as u64;
    let claims = Claims {
        sub: user_id.to_string(),
        device_id: device_id.to_string(),
        exp: now + ttl_mins * 60,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Decode and validate an access token.
pub fn decode_access_token(
    token: &str,
    secret: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let token = encode_access_token("user-123", "device-456", "test-secret", 15).unwrap();
        let claims = decode_access_token(&token, "test-secret").unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.device_id, "device-456");
    }

    #[test]
    fn wrong_secret_fails() {
        let token = encode_access_token("user-123", "device-456", "secret-a", 15).unwrap();
        let result = decode_access_token(&token, "secret-b");
        assert!(result.is_err());
    }
}
