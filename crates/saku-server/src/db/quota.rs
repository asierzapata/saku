use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StorageQuota {
    pub user_id: String,
    pub used_bytes: i64,
}

/// Get the current storage usage for a user.
pub fn get_quota(conn: &Connection, user_id: &str) -> Result<StorageQuota, rusqlite::Error> {
    conn.query_row(
        "SELECT user_id, used_bytes FROM storage_quota WHERE user_id = ?1",
        params![user_id],
        |row| {
            Ok(StorageQuota {
                user_id: row.get(0)?,
                used_bytes: row.get(1)?,
            })
        },
    )
}

/// Add bytes to a user's usage. `delta` can be negative (for deletions).
pub fn update_usage(
    conn: &Connection,
    user_id: &str,
    delta: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE storage_quota SET used_bytes = MAX(0, used_bytes + ?2), last_updated = datetime('now')
         WHERE user_id = ?1",
        params![user_id, delta],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrations::run_migrations, users::create_user};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn quota_starts_at_zero() {
        let conn = setup_db();
        let user_id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        let quota = get_quota(&conn, &user_id).unwrap();
        assert_eq!(quota.used_bytes, 0);
    }

    #[test]
    fn update_usage_adds_bytes() {
        let conn = setup_db();
        let user_id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        update_usage(&conn, &user_id, 1024).unwrap();
        let quota = get_quota(&conn, &user_id).unwrap();
        assert_eq!(quota.used_bytes, 1024);
    }

    #[test]
    fn usage_does_not_go_negative() {
        let conn = setup_db();
        let user_id = create_user(&conn, "test@example.com", "$hash$").unwrap();
        update_usage(&conn, &user_id, -500).unwrap();
        let quota = get_quota(&conn, &user_id).unwrap();
        assert_eq!(quota.used_bytes, 0);
    }
}
