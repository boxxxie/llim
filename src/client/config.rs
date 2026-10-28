use compact_str::CompactString;

use crate::config::{LlimConfig, resolve_api_key};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub api_key: Option<CompactString>,
    pub teams: Vec<CompactString>,
    pub endpoint: String,
    pub delta_poll_seconds: u64,
    pub full_reconcile_minutes: u64,
    pub max_pages_per_poll: Option<u32>,
    pub max_age_days: Option<u32>,
    pub offline: bool,
}

impl ClientConfig {
    pub fn from_llim_config(cfg: &LlimConfig, offline: bool) -> Self {
        let resolved_key = resolve_api_key(cfg).map(|s| CompactString::from(s.as_str()));
        Self {
            api_key: resolved_key,
            teams: cfg.linear.teams.clone(),
            endpoint: "https://api.linear.app/graphql".to_string(),
            delta_poll_seconds: cfg.polling.delta_poll_seconds.max(20),
            full_reconcile_minutes: cfg.polling.full_reconcile_minutes.max(1),
            max_pages_per_poll: cfg.polling.max_pages_per_poll,
            max_age_days: cfg.polling.max_age_days,
            offline,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }
}
