use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::config::ConfigStore;
use crate::models::{ProxyLogEntry, ProxyStats, ProxyStatus};

use super::conversion::{active_provider, gateway_models_response};
use super::listener;

const MAX_PROXY_LOGS: usize = 200;

#[derive(Clone, Default)]
pub struct ProxyRuntime {
    server: Arc<Mutex<Option<ProxyServerHandle>>>,
    pub(super) telemetry: Arc<ProxyTelemetry>,
}

#[derive(Default)]
pub(super) struct ProxyTelemetry {
    stats: Mutex<ProxyStatsState>,
    logs: Mutex<Vec<ProxyLogEntry>>,
}

#[derive(Debug, Clone)]
struct ProxyStatsState {
    total: u64,
    success: u64,
    failed: u64,
    today: u64,
    day: u64,
}

impl Default for ProxyStatsState {
    fn default() -> Self {
        Self {
            total: 0,
            success: 0,
            failed: 0,
            today: 0,
            day: current_day(),
        }
    }
}

impl ProxyTelemetry {
    pub(super) fn record(&self, success: bool) {
        let Ok(mut stats) = self.stats.lock() else {
            return;
        };
        let day = current_day();
        if stats.day != day {
            stats.day = day;
            stats.today = 0;
        }
        stats.total += 1;
        stats.today += 1;
        if success {
            stats.success += 1;
        } else {
            stats.failed += 1;
        }
    }

    pub(super) fn log(&self, level: &str, message: impl Into<String>) {
        let Ok(mut logs) = self.logs.lock() else {
            return;
        };
        logs.push(ProxyLogEntry {
            time: current_time_label(),
            level: level.to_string(),
            message: message.into(),
        });
        if logs.len() > MAX_PROXY_LOGS {
            let excess = logs.len() - MAX_PROXY_LOGS;
            logs.drain(0..excess);
        }
    }

    pub(super) fn stats(&self) -> ProxyStats {
        let Ok(stats) = self.stats.lock() else {
            return ProxyStats::default();
        };
        ProxyStats {
            total: stats.total,
            success: stats.success,
            failed: stats.failed,
            today: if stats.day == current_day() {
                stats.today
            } else {
                0
            },
        }
    }

    pub(super) fn logs(&self) -> Vec<ProxyLogEntry> {
        self.logs
            .lock()
            .map(|logs| logs.clone())
            .unwrap_or_default()
    }

    pub(super) fn clear_logs(&self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
    }
}

fn current_day() -> u64 {
    current_unix_secs() / 86_400
}

fn current_time_label() -> String {
    let seconds = current_unix_secs() % 86_400;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

struct ProxyServerHandle {
    port: u16,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for ProxyServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl ProxyRuntime {
    pub fn start(&self, requested_port: u16) -> Result<u16, String> {
        let mut guard = self
            .server
            .lock()
            .map_err(|_| "Proxy runtime lock is poisoned".to_string())?;
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.port);
        }

        let listener = TcpListener::bind(("127.0.0.1", requested_port))
            .map_err(|error| format!("Failed to bind Rust proxy listener: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("Failed to configure Rust proxy listener: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("Failed to read Rust proxy port: {error}"))?
            .port();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let telemetry = Arc::clone(&self.telemetry);
        let join = thread::spawn(move || listener::run_listener(listener, thread_stop, telemetry));
        *guard = Some(ProxyServerHandle {
            port,
            stop,
            join: Some(join),
        });
        Ok(port)
    }

    pub fn stop(&self) -> Result<bool, String> {
        let mut guard = self
            .server
            .lock()
            .map_err(|_| "Proxy runtime lock is poisoned".to_string())?;
        Ok(guard.take().is_some())
    }

    pub fn running_port(&self) -> Option<u16> {
        self.server
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|handle| handle.port))
    }

    pub fn stats(&self) -> ProxyStats {
        self.telemetry.stats()
    }

    pub fn logs(&self) -> Vec<ProxyLogEntry> {
        self.telemetry.logs()
    }

    pub fn clear_logs(&self) {
        self.telemetry.clear_logs();
    }
}

pub fn start_proxy_listener(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    let config = ConfigStore::default()?.load_config()?;
    runtime.start(config.settings.proxy_port)?;
    proxy_status(runtime)
}

pub fn stop_proxy_listener(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    runtime.stop()?;
    proxy_status(runtime)
}

pub fn proxy_status(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    let config = ConfigStore::default()?.load_config()?;
    let running_port = runtime.running_port();
    Ok(ProxyStatus {
        running: running_port.is_some(),
        port: running_port.unwrap_or(config.settings.proxy_port),
        active_provider_id: config.active_provider,
        has_gateway_key: config
            .gateway_api_key
            .as_deref()
            .is_some_and(|key| !key.is_empty()),
        implemented: true,
        stats: runtime.stats(),
        message: if running_port.is_some() {
            "Rust proxy listener is running; non-streaming and streaming forwarding are available."
                .to_string()
        } else {
            "Rust proxy forwarding is implemented; HTTP listener is stopped.".to_string()
        },
    })
}

pub fn gateway_models_for_active_provider() -> Result<Value, String> {
    let config = ConfigStore::default()?.load_config()?;
    let provider = active_provider(&config);
    let providers = if config.settings.expose_all_provider_models {
        Some(config.providers.as_slice())
    } else {
        None
    };
    Ok(gateway_models_response(
        provider,
        providers,
        config.settings.expose_all_provider_models,
    ))
}

pub fn proxy_logs(runtime: &ProxyRuntime) -> Vec<ProxyLogEntry> {
    runtime.logs()
}

pub fn clear_proxy_logs(runtime: &ProxyRuntime) -> bool {
    runtime.clear_logs();
    true
}
