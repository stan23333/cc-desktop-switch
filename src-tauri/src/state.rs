use std::time::{SystemTime, UNIX_EPOCH};

use crate::proxy::ProxyRuntime;
use crate::update::UpdateRuntime;

#[derive(Clone)]
pub struct AppState {
    started_at_ms: u128,
    proxy_runtime: ProxyRuntime,
    update_runtime: UpdateRuntime,
}

impl AppState {
    pub fn new() -> Self {
        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        Self {
            started_at_ms,
            proxy_runtime: ProxyRuntime::default(),
            update_runtime: UpdateRuntime::default(),
        }
    }

    pub fn started_at_ms(&self) -> u128 {
        self.started_at_ms
    }

    pub fn proxy_runtime(&self) -> &ProxyRuntime {
        &self.proxy_runtime
    }

    pub fn update_runtime(&self) -> &UpdateRuntime {
        &self.update_runtime
    }
}
