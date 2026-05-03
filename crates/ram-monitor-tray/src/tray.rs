use std::path::PathBuf;

use anyhow::{Context, Result};
use ksni::menu::StandardItem;
use ksni::{MenuItem, ToolTip, Tray};
use ram_monitor_core::{Process, Snapshot};

use crate::icon::{IconRenderer, RamIconData};

const REPO_URL: &str = "https://github.com/maximofn/ram_monitor";
const COFFEE_URL: &str = "https://www.buymeacoffee.com/maximofn";
const ICON_BASENAME: &str = "ram-monitor-tray";
const PROCESSES_IN_MENU: usize = 10;

#[derive(Debug, Clone)]
pub enum State {
    Connecting,
    Connected(Snapshot),
    Disconnected(String),
}

pub struct RamTray {
    renderer: IconRenderer,
    backend_url: String,
    state: State,
    icon_dir: PathBuf,
    /// Counter that increments on every redraw so the panel sees a new
    /// `IconName` and reloads the file from disk (matches what AppIndicator's
    /// `set_icon_full` does internally — GNOME-shell otherwise caches by name).
    generation: u64,
    current_icon_name: String,
}

impl RamTray {
    pub fn new(renderer: IconRenderer, backend_url: String, icon_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&icon_dir)
            .with_context(|| format!("creating icon dir {}", icon_dir.display()))?;
        // Wipe any stale icons left by a previous run so the cache stays bounded.
        if let Ok(entries) = std::fs::read_dir(&icon_dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ICON_BASENAME)
                {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let mut tray = Self {
            renderer,
            backend_url,
            state: State::Connecting,
            icon_dir,
            generation: 0,
            current_icon_name: String::new(),
        };
        tray.refresh_icon_file();
        Ok(tray)
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.refresh_icon_file();
    }

    fn refresh_icon_file(&mut self) {
        let png = match self.renderer.render_png(self.icon_data(), self.connected()) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(error = %err, "failed to render icon PNG");
                return;
            }
        };
        self.generation = self.generation.wrapping_add(1);
        let new_name = format!("{ICON_BASENAME}-{}", self.generation);
        let new_path = self.icon_dir.join(format!("{new_name}.png"));
        if let Err(err) = std::fs::write(&new_path, &png) {
            tracing::warn!(error = %err, path = %new_path.display(), "failed to write icon PNG");
            return;
        }

        // Drop the previous frame so the cache directory does not grow.
        if !self.current_icon_name.is_empty() {
            let old = self
                .icon_dir
                .join(format!("{}.png", self.current_icon_name));
            let _ = std::fs::remove_file(old);
        }
        self.current_icon_name = new_name;
    }

    fn icon_data(&self) -> Option<RamIconData> {
        match &self.state {
            State::Connected(snap) => Some(RamIconData {
                used_bytes: snap.memory.used_bytes,
                total_bytes: snap.memory.total_bytes,
                swap_percent: snap.swap.used_percent(),
            }),
            _ => None,
        }
    }

    fn connected(&self) -> bool {
        matches!(self.state, State::Connected(_))
    }
}

impl Tray for RamTray {
    fn id(&self) -> String {
        "ram-monitor".to_string()
    }

    fn title(&self) -> String {
        "RAM Monitor".to_string()
    }

    fn icon_name(&self) -> String {
        self.current_icon_name.clone()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_dir.to_string_lossy().into_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        let title = "RAM Monitor".to_string();
        let description = match &self.state {
            State::Connecting => format!("Connecting to {}", self.backend_url),
            State::Connected(snap) => format!(
                "{:.1} / {:.0} GiB used ({:.0}%)",
                bytes_to_gib_f(snap.memory.used_bytes),
                bytes_to_gib_f(snap.memory.total_bytes),
                snap.memory.used_percent(),
            ),
            State::Disconnected(err) => format!("Backend offline: {err}"),
        };
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title,
            description,
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        match &self.state {
            State::Connecting => {
                items.push(disabled_item(format!(
                    "Connecting to {}…",
                    self.backend_url
                )));
                items.push(MenuItem::Separator);
            }
            State::Disconnected(err) => {
                items.push(disabled_item(format!("Backend offline: {err}")));
                items.push(disabled_item(format!("Backend: {}", self.backend_url)));
                items.push(MenuItem::Separator);
            }
            State::Connected(snap) => {
                items.push(disabled_item(format!(
                    "Total: {:.2} GiB",
                    bytes_to_gib_f(snap.memory.total_bytes)
                )));
                items.push(disabled_item(format!(
                    "Used: {:.2} GiB ({:.1}%)",
                    bytes_to_gib_f(snap.memory.used_bytes),
                    snap.memory.used_percent()
                )));
                items.push(disabled_item(format!(
                    "Available: {:.2} GiB",
                    bytes_to_gib_f(snap.memory.available_bytes)
                )));
                items.push(disabled_item(format!(
                    "Free: {:.2} GiB",
                    bytes_to_gib_f(snap.memory.free_bytes)
                )));
                items.push(disabled_item(format!(
                    "Cached: {:.2} GiB",
                    bytes_to_gib_f(snap.memory.cached_bytes)
                )));
                items.push(disabled_item(format!(
                    "Buffers: {:.2} GiB",
                    bytes_to_gib_f(snap.memory.buffers_bytes)
                )));

                if snap.swap.total_bytes > 0 {
                    items.push(MenuItem::Separator);
                    items.push(disabled_item(format!(
                        "Swap: {:.2} / {:.2} GiB ({:.1}%)",
                        bytes_to_gib_f(snap.swap.used_bytes),
                        bytes_to_gib_f(snap.swap.total_bytes),
                        snap.swap.used_percent(),
                    )));
                }

                if !snap.processes.is_empty() {
                    items.push(MenuItem::Separator);
                    items.push(disabled_item("Top processes by RSS".into()));
                    for proc in snap.processes.iter().take(PROCESSES_IN_MENU) {
                        items.push(disabled_item(format_process(proc)));
                    }
                }

                items.push(MenuItem::Separator);
                items.push(disabled_item(format!("Backend: {}", self.backend_url)));
                items.push(disabled_item(format!(
                    "Updated: {}",
                    short_time(&snap.timestamp)
                )));
                items.push(MenuItem::Separator);
            }
        }

        items.push(MenuItem::Standard(StandardItem {
            label: "Repository".into(),
            activate: Box::new(|_| open_url(REPO_URL)),
            ..Default::default()
        }));
        items.push(MenuItem::Standard(StandardItem {
            label: "Buy me a coffee".into(),
            activate: Box::new(|_| open_url(COFFEE_URL)),
            ..Default::default()
        }));
        items.push(MenuItem::Separator);
        items.push(MenuItem::Standard(StandardItem {
            label: "Quit".into(),
            activate: Box::new(|_| std::process::exit(0)),
            ..Default::default()
        }));

        items
    }
}

fn format_process(proc: &Process) -> String {
    format!(
        "  {:>6} {} — {:.2} GiB ({:.1}%)",
        proc.pid,
        proc.name,
        bytes_to_gib_f(proc.rss_bytes),
        proc.memory_percent
    )
}

fn disabled_item(label: String) -> MenuItem<RamTray> {
    MenuItem::Standard(StandardItem {
        label,
        enabled: false,
        ..Default::default()
    })
}

fn open_url(url: &str) {
    if let Err(err) = open::that(url) {
        tracing::warn!(%url, error = %err, "could not open url");
    }
}

fn bytes_to_gib_f(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

fn short_time(rfc3339: &str) -> &str {
    rfc3339
        .split('T')
        .nth(1)
        .and_then(|s| s.split('.').next())
        .unwrap_or(rfc3339)
}
