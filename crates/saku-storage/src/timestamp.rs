use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridTimestamp {
    pub wall_ms: i64,
    pub lamport: u64,
    pub device_id: String,
}

impl HybridTimestamp {
    /// Create a new HybridTimestamp using the current wall clock time.
    pub fn now(lamport: u64, device_id: String) -> Self {
        Self {
            wall_ms: jiff::Timestamp::now().as_millisecond(),
            lamport,
            device_id,
        }
    }

    /// Create a HybridTimestamp from an existing jiff Timestamp (useful for migrations).
    pub fn from_jiff(ts: jiff::Timestamp, lamport: u64, device_id: String) -> Self {
        Self {
            wall_ms: ts.as_millisecond(),
            lamport,
            device_id,
        }
    }
}

impl Default for HybridTimestamp {
    fn default() -> Self {
        Self {
            wall_ms: 0,
            lamport: 0,
            device_id: String::new(),
        }
    }
}

impl PartialEq for HybridTimestamp {
    fn eq(&self, other: &Self) -> bool {
        self.wall_ms == other.wall_ms
            && self.lamport == other.lamport
            && self.device_id == other.device_id
    }
}

impl Eq for HybridTimestamp {}

impl PartialOrd for HybridTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HybridTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.wall_ms
            .cmp(&other.wall_ms)
            .then_with(|| self.lamport.cmp(&other.lamport))
            .then_with(|| self.device_id.cmp(&other.device_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_wall_ms_takes_priority() {
        let earlier = HybridTimestamp {
            wall_ms: 100,
            lamport: 5,
            device_id: "z".into(),
        };
        let later = HybridTimestamp {
            wall_ms: 200,
            lamport: 1,
            device_id: "a".into(),
        };
        assert!(earlier < later);
    }

    #[test]
    fn ordering_lamport_breaks_wall_ms_tie() {
        let low = HybridTimestamp {
            wall_ms: 100,
            lamport: 1,
            device_id: "z".into(),
        };
        let high = HybridTimestamp {
            wall_ms: 100,
            lamport: 2,
            device_id: "a".into(),
        };
        assert!(low < high);
    }

    #[test]
    fn ordering_device_id_breaks_lamport_tie() {
        let a = HybridTimestamp {
            wall_ms: 100,
            lamport: 1,
            device_id: "aaa".into(),
        };
        let b = HybridTimestamp {
            wall_ms: 100,
            lamport: 1,
            device_id: "bbb".into(),
        };
        assert!(a < b);
    }

    #[test]
    fn equality() {
        let ts1 = HybridTimestamp {
            wall_ms: 100,
            lamport: 1,
            device_id: "dev1".into(),
        };
        let ts2 = HybridTimestamp {
            wall_ms: 100,
            lamport: 1,
            device_id: "dev1".into(),
        };
        assert_eq!(ts1, ts2);
    }

    #[test]
    fn default_is_zero() {
        let ts = HybridTimestamp::default();
        assert_eq!(ts.wall_ms, 0);
        assert_eq!(ts.lamport, 0);
        assert_eq!(ts.device_id, "");
    }

    #[test]
    fn serde_roundtrip() {
        let ts = HybridTimestamp {
            wall_ms: 1234567890,
            lamport: 42,
            device_id: "device-abc".into(),
        };
        let json = serde_json::to_string(&ts).unwrap();
        let deserialized: HybridTimestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, deserialized);
    }

    #[test]
    fn from_jiff_produces_correct_wall_ms() {
        let jiff_ts = jiff::Timestamp::from_millisecond(1_700_000_000_000).unwrap();
        let ts = HybridTimestamp::from_jiff(jiff_ts, 5, "dev".into());
        assert_eq!(ts.wall_ms, 1_700_000_000_000);
        assert_eq!(ts.lamport, 5);
        assert_eq!(ts.device_id, "dev");
    }
}
