use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub quota_bytes: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Device {
    pub id: String,
    pub user_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RefreshToken {
    pub token_hash: String,
    pub device_id: String,
    pub user_id: String,
    pub expires_at: String,
    pub revoked: bool,
}

/// Insert a new user. Returns the user ID.
pub fn create_user(
    conn: &Connection,
    email: &str,
    password_hash: &str,
) -> Result<String, rusqlite::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO users (id, email, password_hash) VALUES (?1, ?2, ?3)",
        params![id, email, password_hash],
    )?;
    // Initialize storage quota
    conn.execute(
        "INSERT INTO storage_quota (user_id) VALUES (?1)",
        params![id],
    )?;
    Ok(id)
}

/// Find a user by email.
pub fn find_user_by_email(conn: &Connection, email: &str) -> Result<Option<User>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT id, email, password_hash, quota_bytes FROM users WHERE email = ?1")?;
    let mut rows = stmt.query(params![email])?;
    match rows.next()? {
        Some(row) => Ok(Some(User {
            id: row.get(0)?,
            email: row.get(1)?,
            password_hash: row.get(2)?,
            quota_bytes: row.get(3)?,
        })),
        None => Ok(None),
    }
}

/// Upsert a device (create or update last_seen_at).
pub fn upsert_device(
    conn: &Connection,
    device_id: &str,
    user_id: &str,
    device_name: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO devices (id, user_id, device_name, last_seen_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET last_seen_at = datetime('now'), device_name = ?3",
        params![device_id, user_id, device_name],
    )?;
    Ok(())
}

/// Store a refresh token hash.
pub fn store_refresh_token(
    conn: &Connection,
    token_hash: &str,
    device_id: &str,
    user_id: &str,
    expires_at: &str,
) -> Result<(), rusqlite::Error> {
    // Revoke any existing tokens for this device
    conn.execute(
        "UPDATE refresh_tokens SET revoked = 1 WHERE device_id = ?1 AND revoked = 0",
        params![device_id],
    )?;
    conn.execute(
        "INSERT INTO refresh_tokens (token_hash, device_id, user_id, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![token_hash, device_id, user_id, expires_at],
    )?;
    Ok(())
}

/// Validate a refresh token hash. Returns the token record if valid.
pub fn validate_refresh_token(
    conn: &Connection,
    token_hash: &str,
) -> Result<Option<RefreshToken>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT token_hash, device_id, user_id, expires_at, revoked
         FROM refresh_tokens
         WHERE token_hash = ?1 AND revoked = 0 AND expires_at > datetime('now')",
    )?;
    let mut rows = stmt.query(params![token_hash])?;
    match rows.next()? {
        Some(row) => Ok(Some(RefreshToken {
            token_hash: row.get(0)?,
            device_id: row.get(1)?,
            user_id: row.get(2)?,
            expires_at: row.get(3)?,
            revoked: row.get::<_, i32>(4)? != 0,
        })),
        None => Ok(None),
    }
}

/// Revoke all refresh tokens for a device.
pub fn revoke_device_tokens(
    conn: &Connection,
    device_id: &str,
    user_id: &str,
) -> Result<u64, rusqlite::Error> {
    let deleted = conn.execute(
        "DELETE FROM refresh_tokens WHERE device_id = ?1 AND user_id = ?2",
        params![device_id, user_id],
    )?;
    conn.execute(
        "DELETE FROM devices WHERE id = ?1 AND user_id = ?2",
        params![device_id, user_id],
    )?;
    Ok(deleted as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn create_and_find_user() {
        let conn = setup_db();
        let id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        let user = find_user_by_email(&conn, "test@example.com")
            .unwrap()
            .unwrap();
        assert_eq!(user.id, id);
        assert_eq!(user.email, "test@example.com");
    }

    #[test]
    fn duplicate_email_fails() {
        let conn = setup_db();
        create_user(&conn, "test@example.com", "$hash$").unwrap();
        let result = create_user(&conn, "test@example.com", "$hash2$");
        assert!(result.is_err());
    }

    #[test]
    fn device_upsert() {
        let conn = setup_db();
        let user_id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        upsert_device(&conn, "dev-1", &user_id, "MacBook").unwrap();
        upsert_device(&conn, "dev-1", &user_id, "MacBook Pro").unwrap(); // update
    }

    #[test]
    fn refresh_token_lifecycle() {
        let conn = setup_db();
        let user_id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        upsert_device(&conn, "dev-1", &user_id, "MacBook").unwrap();

        store_refresh_token(&conn, "hash123", "dev-1", &user_id, "2099-01-01 00:00:00").unwrap();

        let token = validate_refresh_token(&conn, "hash123").unwrap().unwrap();
        assert_eq!(token.device_id, "dev-1");

        // Revoke
        revoke_device_tokens(&conn, "dev-1", &user_id).unwrap();
        let token = validate_refresh_token(&conn, "hash123").unwrap();
        assert!(token.is_none());
    }
}
