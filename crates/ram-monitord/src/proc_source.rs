use std::fs;

use anyhow::{Context, Result};
use ram_monitor_core::{Memory, Process, Swap};

pub trait RamSource: Send + Sync {
    fn kernel(&self) -> Option<String>;
    /// Sample the totals from /proc/meminfo (or equivalent).
    fn sample_memory(&self) -> Result<(Memory, Swap)>;
    /// Sample top-N processes by RSS. Implementations may return fewer than
    /// `top_n` if the system has fewer processes.
    fn sample_processes(&self, top_n: usize, total_bytes: u64) -> Result<Vec<Process>>;
}

pub struct ProcfsSource {
    kernel: Option<String>,
}

impl ProcfsSource {
    pub fn init() -> Result<Self> {
        let kernel = read_kernel_release().ok();
        // Sanity-check that /proc/meminfo is readable now so we fail at startup
        // rather than per-sample if the host is misconfigured.
        let _ = parse_meminfo(&fs::read_to_string("/proc/meminfo").context("reading /proc/meminfo")?)?;
        Ok(Self { kernel })
    }
}

impl RamSource for ProcfsSource {
    fn kernel(&self) -> Option<String> {
        self.kernel.clone()
    }

    fn sample_memory(&self) -> Result<(Memory, Swap)> {
        let raw = fs::read_to_string("/proc/meminfo").context("reading /proc/meminfo")?;
        parse_meminfo(&raw)
    }

    fn sample_processes(&self, top_n: usize, total_bytes: u64) -> Result<Vec<Process>> {
        if top_n == 0 {
            return Ok(Vec::new());
        }
        let mut all = collect_processes(total_bytes);
        // Sort by RSS desc, ties broken by PID asc for determinism.
        all.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes).then(a.pid.cmp(&b.pid)));
        all.truncate(top_n);
        Ok(all)
    }
}

fn read_kernel_release() -> Result<String> {
    let raw = fs::read_to_string("/proc/sys/kernel/osrelease")
        .context("reading /proc/sys/kernel/osrelease")?;
    Ok(raw.trim().to_string())
}

fn parse_meminfo(raw: &str) -> Result<(Memory, Swap)> {
    let mut total = 0u64;
    let mut free = 0u64;
    let mut available = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;
    let mut have_total = false;

    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let key = match parts.next() {
            Some(k) => k.trim_end_matches(':'),
            None => continue,
        };
        let value: u64 = match parts.next().and_then(|v| v.parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        // /proc/meminfo expresses sizes in kB (kilobytes, 1024 bytes per the
        // kernel's convention — yes, the unit string is wrong, that's history).
        let bytes = value * 1024;
        match key {
            "MemTotal" => {
                total = bytes;
                have_total = true;
            }
            "MemFree" => free = bytes,
            "MemAvailable" => available = bytes,
            "Buffers" => buffers = bytes,
            "Cached" => cached = bytes,
            "SwapTotal" => swap_total = bytes,
            "SwapFree" => swap_free = bytes,
            _ => {}
        }
    }

    if !have_total {
        anyhow::bail!("/proc/meminfo did not contain MemTotal");
    }

    // If MemAvailable is missing (very old kernels, < 3.14), approximate as
    // MemFree + Buffers + Cached. Newer kernels supply it directly and that's
    // strictly better than the approximation.
    let available = if available == 0 {
        free.saturating_add(buffers).saturating_add(cached)
    } else {
        available
    };
    let used = total.saturating_sub(available);

    let memory = Memory {
        total_bytes: total,
        free_bytes: free,
        available_bytes: available,
        buffers_bytes: buffers,
        cached_bytes: cached,
        used_bytes: used,
    };
    let swap = Swap {
        total_bytes: swap_total,
        free_bytes: swap_free,
        used_bytes: swap_total.saturating_sub(swap_free),
    };
    Ok((memory, swap))
}

fn collect_processes(total_bytes: u64) -> Vec<Process> {
    let mut out = Vec::new();
    let entries = match fs::read_dir("/proc") {
        Ok(it) => it,
        Err(err) => {
            tracing::warn!(error = %err, "could not read /proc");
            return out;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        // Many /proc/<pid> entries vanish between readdir and read (short-lived
        // processes). Silently skip those — they're racy by design.
        let status_raw = match fs::read_to_string(format!("/proc/{pid}/status")) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed = parse_status(&status_raw);
        let (Some(name), Some(rss_kb), Some(vsz_kb)) = (parsed.name, parsed.rss_kb, parsed.vsz_kb)
        else {
            continue;
        };
        let rss_bytes = rss_kb * 1024;
        let memory_percent = if total_bytes == 0 {
            0.0
        } else {
            (rss_bytes as f32 / total_bytes as f32) * 100.0
        };
        out.push(Process {
            pid,
            name,
            rss_bytes,
            vsz_bytes: vsz_kb * 1024,
            memory_percent,
        });
    }
    out
}

#[derive(Default)]
struct ParsedStatus {
    name: Option<String>,
    rss_kb: Option<u64>,
    vsz_kb: Option<u64>,
}

fn parse_status(raw: &str) -> ParsedStatus {
    let mut p = ParsedStatus::default();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            p.name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("VmRSS:") {
            p.rss_kb = first_number(rest);
        } else if let Some(rest) = line.strip_prefix("VmSize:") {
            p.vsz_kb = first_number(rest);
        }
        // Once we have all three we can stop scanning to save cycles on
        // long /proc/<pid>/status files.
        if p.name.is_some() && p.rss_kb.is_some() && p.vsz_kb.is_some() {
            break;
        }
    }
    p
}

fn first_number(s: &str) -> Option<u64> {
    s.split_whitespace().next().and_then(|t| t.parse().ok())
}

pub struct MockSource {
    pub memory: Memory,
    pub swap: Swap,
    pub processes: Vec<Process>,
    pub kernel: Option<String>,
}

impl MockSource {
    pub fn synthetic() -> Self {
        let total = 32 * 1024 * 1024 * 1024;
        let used = 16 * 1024 * 1024 * 1024;
        Self {
            memory: Memory {
                total_bytes: total,
                free_bytes: 4 * 1024 * 1024 * 1024,
                available_bytes: total - used,
                buffers_bytes: 256 * 1024 * 1024,
                cached_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: used,
            },
            swap: Swap {
                total_bytes: 8 * 1024 * 1024 * 1024,
                free_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 0,
            },
            processes: vec![
                Process {
                    pid: 1234,
                    name: "firefox".into(),
                    rss_bytes: 3 * 1024 * 1024 * 1024,
                    vsz_bytes: 8 * 1024 * 1024 * 1024,
                    memory_percent: 9.4,
                },
                Process {
                    pid: 4321,
                    name: "code".into(),
                    rss_bytes: 1_500_000_000,
                    vsz_bytes: 4_000_000_000,
                    memory_percent: 4.4,
                },
            ],
            kernel: Some("mock-kernel".into()),
        }
    }
}

impl RamSource for MockSource {
    fn kernel(&self) -> Option<String> {
        self.kernel.clone()
    }

    fn sample_memory(&self) -> Result<(Memory, Swap)> {
        Ok((self.memory, self.swap))
    }

    fn sample_processes(&self, top_n: usize, _total_bytes: u64) -> Result<Vec<Process>> {
        let mut out = self.processes.clone();
        out.truncate(top_n);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MEMINFO: &str = "\
MemTotal:       32761772 kB
MemFree:        27153804 kB
MemAvailable:   29385272 kB
Buffers:          257048 kB
Cached:          2268844 kB
SwapTotal:       2097148 kB
SwapFree:        2097148 kB
";

    #[test]
    fn meminfo_parses_canonical_format() {
        let (mem, swap) = parse_meminfo(SAMPLE_MEMINFO).unwrap();
        assert_eq!(mem.total_bytes, 32_761_772 * 1024);
        assert_eq!(mem.free_bytes, 27_153_804 * 1024);
        assert_eq!(mem.available_bytes, 29_385_272 * 1024);
        assert_eq!(mem.used_bytes, mem.total_bytes - mem.available_bytes);
        assert_eq!(swap.total_bytes, 2_097_148 * 1024);
        assert_eq!(swap.used_bytes, 0);
    }

    #[test]
    fn meminfo_falls_back_when_available_missing() {
        let raw = "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 200 kB\nCached: 300 kB\n";
        let (mem, _) = parse_meminfo(raw).unwrap();
        // 100 + 200 + 300 = 600 kB
        assert_eq!(mem.available_bytes, 600 * 1024);
        assert_eq!(mem.used_bytes, mem.total_bytes - mem.available_bytes);
    }

    #[test]
    fn meminfo_rejects_input_without_total() {
        assert!(parse_meminfo("MemFree: 0 kB\n").is_err());
    }

    #[test]
    fn parse_status_picks_known_fields() {
        let raw = "Name:\tfirefox\nState:\tS\nVmSize:\t   8388608 kB\nVmRSS:\t   2097152 kB\n";
        let parsed = parse_status(raw);
        assert_eq!(parsed.name.as_deref(), Some("firefox"));
        assert_eq!(parsed.vsz_kb, Some(8_388_608));
        assert_eq!(parsed.rss_kb, Some(2_097_152));
    }

    #[test]
    fn mock_source_returns_seeded_data() {
        let m = MockSource::synthetic();
        let (mem, swap) = m.sample_memory().unwrap();
        assert!(mem.total_bytes > 0);
        assert!(swap.total_bytes > 0);
        let procs = m.sample_processes(10, mem.total_bytes).unwrap();
        assert!(!procs.is_empty());
    }
}
