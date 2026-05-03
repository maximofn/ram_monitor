use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub timestamp: String,
    pub host: String,
    pub kernel: Option<String>,
    pub memory: Memory,
    pub swap: Swap,
    pub processes: Vec<Process>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Memory {
    pub total_bytes: u64,
    /// `MemFree` from /proc/meminfo: pages the kernel has not allocated at all.
    pub free_bytes: u64,
    /// `MemAvailable` from /proc/meminfo: what the kernel estimates is reclaimable
    /// for new allocations without swapping. This is the number end-users care
    /// about — `free_bytes` alone underreports because it ignores reclaimable
    /// page cache.
    pub available_bytes: u64,
    pub buffers_bytes: u64,
    pub cached_bytes: u64,
    /// total - available, i.e. memory unlikely to be reclaimable.
    pub used_bytes: u64,
}

impl Memory {
    pub fn used_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f32 / self.total_bytes as f32) * 100.0
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Swap {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
}

impl Swap {
    pub fn used_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f32 / self.total_bytes as f32) * 100.0
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    /// Resident Set Size: physical RAM the process holds.
    pub rss_bytes: u64,
    /// Virtual memory size; useful to spot bloated address spaces even
    /// when RSS is small (e.g. JVMs).
    pub vsz_bytes: u64,
    pub memory_percent: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrips_through_json() {
        let snap = Snapshot {
            timestamp: "2026-05-03T12:00:00Z".into(),
            host: "carbon".into(),
            kernel: Some("6.5.0".into()),
            memory: Memory {
                total_bytes: 32 * 1024 * 1024 * 1024,
                free_bytes: 4 * 1024 * 1024 * 1024,
                available_bytes: 16 * 1024 * 1024 * 1024,
                buffers_bytes: 256 * 1024 * 1024,
                cached_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 16 * 1024 * 1024 * 1024,
            },
            swap: Swap {
                total_bytes: 8 * 1024 * 1024 * 1024,
                free_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 0,
            },
            processes: vec![Process {
                pid: 1234,
                name: "firefox".into(),
                rss_bytes: 2_500_000_000,
                vsz_bytes: 8_000_000_000,
                memory_percent: 7.5,
            }],
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }

    #[test]
    fn used_percent_handles_zero_total() {
        assert_eq!(Memory::default().used_percent(), 0.0);
        assert_eq!(Swap::default().used_percent(), 0.0);
    }

    #[test]
    fn used_percent_computes_correctly() {
        let m = Memory {
            total_bytes: 100,
            used_bytes: 50,
            ..Default::default()
        };
        assert!((m.used_percent() - 50.0).abs() < f32::EPSILON);
    }
}
