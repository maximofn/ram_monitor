mod config;
mod http;
mod proc_source;
mod sampler;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;
use proc_source::{MockSource, ProcfsSource, RamSource};
use ram_monitor_core::Snapshot;
use sampler::{build_snapshot, empty_snapshot};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::parse();
    init_tracing(&cfg.log_level);

    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "localhost".to_string());

    let source: Arc<dyn RamSource> = if cfg.mock {
        tracing::warn!("running with MOCK RAM source");
        Arc::new(MockSource::synthetic())
    } else {
        Arc::new(
            ProcfsSource::init()
                .context("failed to initialise procfs source; is /proc mounted?")?,
        )
    };

    let initial: Snapshot = match source.sample_memory() {
        Ok(_) => build_snapshot(&host, source.as_ref(), cfg.max_processes),
        Err(err) => {
            tracing::warn!(error = %err, "initial sample failed; serving empty snapshot");
            empty_snapshot(&host, source.kernel())
        }
    };
    let (tx, rx) = watch::channel(initial);

    sampler::spawn(source, host, cfg.sample_interval_ms, cfg.max_processes, tx);

    let state = http::AppState {
        started_at: Instant::now(),
        snapshot_rx: rx,
    };
    let app = http::build_router(state);

    let addr = SocketAddr::new(cfg.bind, cfg.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!(%addr, "ram-monitord listening");

    // axum's `with_graceful_shutdown` waits for connections to drain. SSE
    // streams are infinite by design, so a graceful shutdown would never
    // complete and `systemctl stop` would hang. Instead, race the server
    // against the signal and abort everything when SIGINT/SIGTERM arrives.
    tokio::select! {
        result = axum::serve(listener, app) => {
            result.context("HTTP server error")?;
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown requested; aborting in-flight SSE streams");
        }
    }

    tracing::info!("shutdown complete");
    Ok(())
}

fn init_tracing(directive: &str) {
    let filter = EnvFilter::try_new(directive).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "could not install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received"),
        _ = terminate => tracing::info!("SIGTERM received"),
    }
}
