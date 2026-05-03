use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use ram_monitor_core::{Memory, Snapshot, Swap};
use tokio::sync::watch;
use tokio::time::{interval, MissedTickBehavior};

use crate::proc_source::RamSource;

pub fn empty_snapshot(host: &str, kernel: Option<String>) -> Snapshot {
    Snapshot {
        timestamp: Utc::now().to_rfc3339(),
        host: host.to_string(),
        kernel,
        memory: Memory::default(),
        swap: Swap::default(),
        processes: Vec::new(),
    }
}

pub fn build_snapshot(host: &str, source: &dyn RamSource, max_processes: usize) -> Snapshot {
    let (memory, swap) = match source.sample_memory() {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(error = %err, "memory sample failed; serving zeros");
            (Memory::default(), Swap::default())
        }
    };
    let processes = match source.sample_processes(max_processes, memory.total_bytes) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(error = %err, "process sample failed; serving empty list");
            Vec::new()
        }
    };
    Snapshot {
        timestamp: Utc::now().to_rfc3339(),
        host: host.to_string(),
        kernel: source.kernel(),
        memory,
        swap,
        processes,
    }
}

pub fn spawn(
    source: Arc<dyn RamSource>,
    host: String,
    interval_ms: u64,
    max_processes: usize,
    tx: watch::Sender<Snapshot>,
) {
    tokio::spawn(async move {
        let period = Duration::from_millis(interval_ms.max(50));
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            let snapshot = build_snapshot(&host, source.as_ref(), max_processes);
            if tx.send(snapshot).is_err() {
                tracing::info!("snapshot channel closed; sampler exiting");
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc_source::MockSource;

    #[test]
    fn build_snapshot_uses_source_metadata() {
        let source = MockSource::synthetic();
        let snap = build_snapshot("host-x", &source, 5);
        assert_eq!(snap.host, "host-x");
        assert!(snap.memory.total_bytes > 0);
        assert!(!snap.processes.is_empty());
        assert_eq!(snap.kernel.as_deref(), Some("mock-kernel"));
    }

    #[test]
    fn empty_snapshot_has_no_data() {
        let snap = empty_snapshot("h", None);
        assert!(snap.processes.is_empty());
        assert_eq!(snap.memory.total_bytes, 0);
    }
}
