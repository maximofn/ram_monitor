use std::net::IpAddr;

use clap::Parser;
use ram_monitor_core::{DEFAULT_BIND, DEFAULT_PORT};

#[derive(Debug, Clone, Parser)]
#[command(name = "ram-monitord", about = "RAM monitor backend daemon", version)]
pub struct Config {
    #[arg(long, env = "RAM_MONITORD_BIND", default_value = DEFAULT_BIND)]
    pub bind: IpAddr,

    #[arg(long, env = "RAM_MONITORD_PORT", default_value_t = DEFAULT_PORT)]
    pub port: u16,

    #[arg(long, env = "RAM_MONITORD_SAMPLE_INTERVAL_MS", default_value_t = 1000)]
    pub sample_interval_ms: u64,

    /// Top N processes by RSS to include in each snapshot. Set to 0 to skip
    /// the process scan entirely (cheap if only the totals matter).
    #[arg(long, env = "RAM_MONITORD_MAX_PROCESSES", default_value_t = 20)]
    pub max_processes: usize,

    #[arg(long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "RAM_MONITORD_MOCK", default_value_t = false)]
    pub mock: bool,
}
