use std::path::{Path, PathBuf};

use compact_str::CompactString;
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

use crate::{
    domain::ViewKind,
    result::{LlimError, Result},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
    EnvVar(CompactString),
    ConfigFile(CompactString),
    LinearCliFallback(CompactString),
}

impl SecretSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::EnvVar(s) | Self::ConfigFile(s) | Self::LinearCliFallback(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearSection {
    #[serde(default)]
    pub api_key: Option<CompactString>,
    #[serde(default = "default_teams")]
    pub teams: Vec<CompactString>,
}

fn default_teams() -> Vec<CompactString> {
    Vec::new()
}

impl Default for LinearSection {
    fn default() -> Self {
        Self {
            api_key: None,
            teams: default_teams(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewsSection {
    #[serde(default)]
    pub default_view: ViewKind,
    #[serde(default = "default_activity_feed_lines")]
    pub activity_feed_lines: u16,
    #[serde(default)]
    pub show_completed_issues: bool,
}

fn default_activity_feed_lines() -> u16 {
    8
}

impl Default for ViewsSection {
    fn default() -> Self {
        Self {
            default_view: ViewKind::Issues,
            activity_feed_lines: default_activity_feed_lines(),
            show_completed_issues: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollingSection {
    #[serde(default = "default_delta_poll_seconds")]
    pub delta_poll_seconds: u64,
    #[serde(default = "default_full_reconcile_minutes")]
    pub full_reconcile_minutes: u64,
    #[serde(default)]
    pub max_pages_per_poll: Option<u32>,
    #[serde(default)]
    pub max_stored_issues: Option<usize>,
    #[serde(default)]
    pub max_age_days: Option<u32>,
    #[serde(default)]
    pub max_activity_events: Option<usize>,
}

fn default_delta_poll_seconds() -> u64 {
    30
}

fn default_full_reconcile_minutes() -> u64 {
    20
}

impl Default for PollingSection {
    fn default() -> Self {
        Self {
            delta_poll_seconds: default_delta_poll_seconds(),
            full_reconcile_minutes: default_full_reconcile_minutes(),
            max_pages_per_poll: None,
            max_stored_issues: None,
            max_age_days: None,
            max_activity_events: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSection {
    #[serde(default = "default_true")]
    pub quack_server_enabled: bool,
    #[serde(default = "default_quack_port")]
    pub quack_port: u16,
}

fn default_true() -> bool {
    true
}

fn default_quack_port() -> u16 {
    9494
}

impl Default for DatabaseSection {
    fn default() -> Self {
        Self {
            quack_server_enabled: true,
            quack_port: default_quack_port(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsSection {
    #[serde(default = "default_notification_check_seconds")]
    pub notification_check_seconds: u64,
    #[serde(default = "default_true")]
    pub direct_attention_alert: bool,
    #[serde(default = "default_true")]
    pub per_issue_dedup: bool,
    #[serde(default = "default_coalesce_threshold")]
    pub coalesce_threshold: usize,
    #[serde(default = "default_true")]
    pub ignore_self_events: bool,
    #[serde(default = "default_true")]
    pub ignore_bot_events: bool,
    #[serde(default = "default_true")]
    pub desktop_notifications_enabled: bool,
}

fn default_notification_check_seconds() -> u64 {
    10
}

fn default_coalesce_threshold() -> usize {
    3
}

impl Default for NotificationsSection {
    fn default() -> Self {
        Self {
            notification_check_seconds: default_notification_check_seconds(),
            direct_attention_alert: true,
            per_issue_dedup: true,
            coalesce_threshold: default_coalesce_threshold(),
            ignore_self_events: true,
            ignore_bot_events: true,
            desktop_notifications_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectsSection {
    #[serde(default = "default_true")]
    pub animations_enabled: bool,
    #[serde(default = "default_in_app_timeout_ms")]
    pub in_app_notification_timeout_ms: u64,
}

fn default_in_app_timeout_ms() -> u64 {
    5000
}

impl Default for EffectsSection {
    fn default() -> Self {
        Self {
            animations_enabled: true,
            in_app_notification_timeout_ms: default_in_app_timeout_ms(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlimConfig {
    #[serde(default)]
    pub linear: LinearSection,
    #[serde(default)]
    pub views: ViewsSection,
    #[serde(default)]
    pub polling: PollingSection,
    #[serde(default)]
    pub database: DatabaseSection,
    #[serde(default)]
    pub notifications: NotificationsSection,
    #[serde(default)]
    pub effects: EffectsSection,
}

pub fn default_config_path() -> PathBuf {
    if let Some(dirs) = BaseDirs::new() {
        dirs.config_dir().join("llim.toml")
    } else {
        PathBuf::from("llim.toml")
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "llim") {
        proj_dirs.data_local_dir().to_path_buf()
    } else if let Some(dirs) = BaseDirs::new() {
        dirs.data_local_dir().join("llim")
    } else {
        PathBuf::from(".llim-data")
    }
}

pub fn default_db_path() -> PathBuf {
    default_data_dir().join("llim.duckdb")
}

pub fn default_quack_token_path() -> PathBuf {
    default_data_dir().join("quack.token")
}

pub fn load_config(path: &Path) -> Result<LlimConfig> {
    if !path.exists() {
        return Ok(LlimConfig::default());
    }
    confy::load_path(path).map_err(|e| LlimError::ConfigLoadError {
        path: path.to_path_buf(),
        message: e.to_string().into(),
    })
}

/// Resolves the Linear Personal API Key in priority order:
/// 1. `LINEAR_API_KEY` environment variable
/// 2. `linear.api_key` in `~/.config/llim.toml`
/// 3. Read-only fallback to `~/.linear-cli/config.toml` or `~/.config/linear/config.toml`
pub fn resolve_api_key(config: &LlimConfig) -> Option<SecretSource> {
    if let Ok(env_key) = std::env::var("LINEAR_API_KEY") {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            return Some(SecretSource::EnvVar(trimmed.into()));
        }
    }

    if let Some(ref cfg_key) = config.linear.api_key {
        let trimmed = cfg_key.trim();
        if !trimmed.is_empty() {
            return Some(SecretSource::ConfigFile(trimmed.into()));
        }
    }

    if let Some(cli_key) = read_linear_cli_fallback_key() {
        return Some(SecretSource::LinearCliFallback(cli_key));
    }

    None
}

fn read_linear_cli_fallback_key() -> Option<CompactString> {
    static CACHED_CLI_KEY: std::sync::OnceLock<Option<CompactString>> = std::sync::OnceLock::new();
    CACHED_CLI_KEY
        .get_or_init(|| {
            if let Some(base) = BaseDirs::new() {
                let candidates = [
                    base.home_dir().join(".linear-cli").join("config.toml"),
                    base.config_dir().join("linear").join("config.toml"),
                    base.home_dir().join(".linear.toml"),
                ];

                for path in candidates {
                    if let Ok(raw) = std::fs::read_to_string(&path) {
                        if let Ok(val) = raw.parse::<toml::Value>() {
                            if let Some(key) = extract_token_from_toml(&val) {
                                return Some(key);
                            }
                        }
                    }
                }
            }

            if let Ok(out) = std::process::Command::new("python3")
                .arg("-c")
                .arg("from linear_cli.api.auth.core import LinearAuthenticator; print(LinearAuthenticator().get_access_token() or '', end='')")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
            {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout);
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(CompactString::from(trimmed));
                    }
                }
            }

            None
        })
        .clone()
}

fn extract_token_from_toml(val: &toml::Value) -> Option<CompactString> {
    let table = val.as_table()?;
    for field in ["api_key", "token", "linear_token", "apiKey"] {
        if let Some(s) = table.get(field).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.into());
            }
        }
    }
    for (_, sub) in table {
        if let Some(sub_table) = sub.as_table() {
            for field in ["api_key", "token", "linear_token", "apiKey"] {
                if let Some(s) = sub_table.get(field).and_then(|v| v.as_str()) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.into());
                    }
                }
            }
        }
    }
    None
}

pub fn save_config(path: &Path, config: &LlimConfig, persist_api_key: bool) -> Result<()> {
    let mut sanitized = config.clone();
    if !persist_api_key {
        // Never leak an environment-sourced or linear-cli fallback token to disk unless explicitly set in ConfigPopup
        if let Ok(env_key) = std::env::var("LINEAR_API_KEY") {
            if sanitized.linear.api_key.as_deref() == Some(env_key.trim()) {
                sanitized.linear.api_key = None;
            }
        }
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    confy::store_path(path, &sanitized)
        .map_err(|e| LlimError::config_save_error(path.to_path_buf(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
    }

    Ok(())
}
