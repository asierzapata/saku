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
    let device_id = saku_storage::device::get_or_create_device_id()
        .unwrap_or_else(|_| "unknown".to_string());
    HybridTimestamp::now(lamport, device_id)
}
