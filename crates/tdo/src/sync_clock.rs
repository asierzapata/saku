use std::sync::atomic::{AtomicU64, Ordering};

use saku_storage::timestamp::HybridTimestamp;

/// Per-process monotonic Lamport counter.
static LAMPORT: AtomicU64 = AtomicU64::new(1);

/// Generate a fresh `HybridTimestamp` for the current mutation.
///
/// Increments the process-local Lamport counter and reads the device_id
/// from disk (falling back to "unknown" if unavailable).
pub fn next_modified_at() -> HybridTimestamp {
    let lamport = LAMPORT.fetch_add(1, Ordering::Relaxed);
    let device_id =
        saku_storage::device::get_or_create_device_id().unwrap_or_else(|_| "unknown".to_string());
    HybridTimestamp::now(lamport, device_id)
}

pub(crate) fn after(
    previous: &HybridTimestamp,
    now_ms: i64,
    device_id: String,
) -> Result<HybridTimestamp, crate::storage::StorageError> {
    let (wall_ms, lamport) = if now_ms > previous.wall_ms {
        (now_ms, 0)
    } else if let Some(lamport) = previous.lamport.checked_add(1) {
        (previous.wall_ms, lamport)
    } else {
        (
            previous.wall_ms.checked_add(1).ok_or_else(|| {
                crate::storage::StorageError::InvalidData("hybrid clock exhausted".into())
            })?,
            0,
        )
    };
    Ok(HybridTimestamp {
        wall_ms,
        lamport,
        device_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_across_equal_or_rollback_time_and_devices() {
        let previous = HybridTimestamp {
            wall_ms: 100,
            lamport: 7,
            device_id: "z".into(),
        };
        for now in [100, 50] {
            let next = after(&previous, now, "a".into()).unwrap();
            assert_eq!((next.wall_ms, next.lamport), (100, 8));
            assert!(next > previous);
        }
        let next = after(&previous, 101, "a".into()).unwrap();
        assert_eq!((next.wall_ms, next.lamport), (101, 0));
        assert!(next > previous);
    }

    #[test]
    fn counter_overflow_carries_and_total_overflow_fails() {
        let previous = HybridTimestamp {
            wall_ms: 100,
            lamport: u64::MAX,
            device_id: "remote".into(),
        };
        let next = after(&previous, 100, "local".into()).unwrap();
        assert_eq!((next.wall_ms, next.lamport), (101, 0));
        let exhausted = HybridTimestamp {
            wall_ms: i64::MAX,
            ..previous
        };
        assert!(after(&exhausted, i64::MAX, "local".into()).is_err());
    }
}
