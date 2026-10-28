use compact_str::CompactString;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::{
    config::LlimConfig,
    gruvbox::Gruvbox,
    theme::theme,
    ui::{fx::window::Window, popup::utility::CenteredShrink},
};

#[derive(Debug, Clone)]
pub struct ConfigPopup {
    base_config: LlimConfig,
    api_key_input: String,
    api_key_modified: bool,
    teams_csv: String,
    delta_poll_seconds: String,
    full_reconcile_minutes: String,
    activity_feed_lines: String,
    show_completed_issues: bool,
    quack_server_enabled: bool,
    quack_port: String,
    notification_check_seconds: String,
    direct_attention_alert: bool,
    per_issue_dedup: bool,
    coalesce_threshold: String,
    ignore_self_events: bool,
    ignore_bot_events: bool,
    desktop_notifications_enabled: bool,
    animations_enabled: bool,
    selected_index: usize,
}

const FIELD_COUNT: usize = 16;

impl ConfigPopup {
    pub fn new(config: LlimConfig) -> Self {
        let api_key_input = config
            .linear
            .api_key
            .as_deref()
            .unwrap_or("")
            .to_string();
        let teams_csv = config
            .linear
            .teams
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        Self {
            api_key_input,
            api_key_modified: false,
            teams_csv,
            delta_poll_seconds: config.polling.delta_poll_seconds.to_string(),
            full_reconcile_minutes: config.polling.full_reconcile_minutes.to_string(),
            activity_feed_lines: config.views.activity_feed_lines.to_string(),
            show_completed_issues: config.views.show_completed_issues,
            quack_server_enabled: config.database.quack_server_enabled,
            quack_port: config.database.quack_port.to_string(),
            notification_check_seconds: config
                .notifications
                .notification_check_seconds
                .to_string(),
            direct_attention_alert: config.notifications.direct_attention_alert,
            per_issue_dedup: config.notifications.per_issue_dedup,
            coalesce_threshold: config.notifications.coalesce_threshold.to_string(),
            ignore_self_events: config.notifications.ignore_self_events,
            ignore_bot_events: config.notifications.ignore_bot_events,
            desktop_notifications_enabled: config.notifications.desktop_notifications_enabled,
            animations_enabled: config.effects.animations_enabled,
            base_config: config,
            selected_index: 0,
        }
    }

    pub fn select_next_input(&mut self) {
        self.selected_index = (self.selected_index + 1) % FIELD_COUNT;
    }

    pub fn select_previous_input(&mut self) {
        self.selected_index = (self.selected_index + FIELD_COUNT - 1) % FIELD_COUNT;
    }

    pub fn handle_key(&mut self, e: &KeyEvent) {
        match self.selected_index {
            5 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.show_completed_issues = !self.show_completed_issues;
                }
            }
            6 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.quack_server_enabled = !self.quack_server_enabled;
                }
            }
            9 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.direct_attention_alert = !self.direct_attention_alert;
                }
            }
            10 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.per_issue_dedup = !self.per_issue_dedup;
                }
            }
            12 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.ignore_self_events = !self.ignore_self_events;
                }
            }
            13 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.ignore_bot_events = !self.ignore_bot_events;
                }
            }
            14 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.desktop_notifications_enabled = !self.desktop_notifications_enabled;
                }
            }
            15 => {
                if matches!(e.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    self.animations_enabled = !self.animations_enabled;
                }
            }
            idx => {
                let is_api_key = idx == 0;
                let target = match idx {
                    0 => &mut self.api_key_input,
                    1 => &mut self.teams_csv,
                    2 => &mut self.delta_poll_seconds,
                    3 => &mut self.full_reconcile_minutes,
                    4 => &mut self.activity_feed_lines,
                    7 => &mut self.quack_port,
                    8 => &mut self.notification_check_seconds,
                    11 => &mut self.coalesce_threshold,
                    _ => return,
                };
                match e.code {
                    KeyCode::Backspace => {
                        target.pop();
                        if is_api_key {
                            self.api_key_modified = true;
                        }
                    }
                    KeyCode::Char(c) => {
                        target.push(c);
                        if is_api_key {
                            self.api_key_modified = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn to_config(&self) -> (LlimConfig, bool) {
        let mut cfg = self.base_config.clone();
        if self.api_key_modified {
            let trimmed = self.api_key_input.trim();
            cfg.linear.api_key = if trimmed.is_empty() {
                None
            } else {
                Some(CompactString::from(trimmed))
            };
        }
        cfg.linear.teams = self
            .teams_csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(CompactString::from)
            .collect();

        if let Ok(v) = self.delta_poll_seconds.trim().parse::<u64>() {
            cfg.polling.delta_poll_seconds = v.max(20);
        }
        if let Ok(v) = self.full_reconcile_minutes.trim().parse::<u64>() {
            cfg.polling.full_reconcile_minutes = v.max(1);
        }
        if let Ok(v) = self.activity_feed_lines.trim().parse::<u16>() {
            cfg.views.activity_feed_lines = v.clamp(3, 30);
        }
        cfg.views.show_completed_issues = self.show_completed_issues;
        cfg.database.quack_server_enabled = self.quack_server_enabled;
        if let Ok(v) = self.quack_port.trim().parse::<u16>() {
            cfg.database.quack_port = v;
        }
        if let Ok(v) = self.notification_check_seconds.trim().parse::<u64>() {
            cfg.notifications.notification_check_seconds = v.max(2);
        }
        cfg.notifications.direct_attention_alert = self.direct_attention_alert;
        cfg.notifications.per_issue_dedup = self.per_issue_dedup;
        if let Ok(v) = self.coalesce_threshold.trim().parse::<usize>() {
            cfg.notifications.coalesce_threshold = v.max(1);
        }
        cfg.notifications.ignore_self_events = self.ignore_self_events;
        cfg.notifications.ignore_bot_events = self.ignore_bot_events;
        cfg.notifications.desktop_notifications_enabled = self.desktop_notifications_enabled;
        cfg.effects.animations_enabled = self.animations_enabled;

        (cfg, self.api_key_modified)
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(76, 82)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(" Configuration ", theme().border.title),
            Span::styled(" ~/.config/llim.toml ", theme().border_title),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let masked_api = if self.api_key_input.is_empty() {
            "(using LINEAR_API_KEY env or linear-cli)".to_string()
        } else if self.selected_index == 0 {
            self.api_key_input.clone()
        } else {
            let suffix: String = self
                .api_key_input
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("••••••••{suffix}")
        };

        let fields: Vec<(&str, String)> = vec![
            ("Linear API Key", masked_api),
            ("Monitored Teams (CSV)", self.teams_csv.clone()),
            (
                "Delta Poll Interval (sec, min 20)",
                self.delta_poll_seconds.clone(),
            ),
            (
                "Full Reconcile Interval (min)",
                self.full_reconcile_minutes.clone(),
            ),
            (
                "Activity Feed Height (lines)",
                self.activity_feed_lines.clone(),
            ),
            (
                "Show Completed/Canceled Issues",
                bool_str(self.show_completed_issues),
            ),
            (
                "DuckDB Quack Server Enabled",
                bool_str(self.quack_server_enabled),
            ),
            ("DuckDB Quack Port", self.quack_port.clone()),
            (
                "Notification Buffer Check (sec)",
                self.notification_check_seconds.clone(),
            ),
            (
                "Direct Attention Immediate Alert",
                bool_str(self.direct_attention_alert),
            ),
            (
                "Per-Issue Notification Dedup",
                bool_str(self.per_issue_dedup),
            ),
            (
                "Multi-Issue Coalesce Threshold",
                self.coalesce_threshold.clone(),
            ),
            ("Ignore Self Events", bool_str(self.ignore_self_events)),
            ("Ignore Bot Events", bool_str(self.ignore_bot_events)),
            (
                "Desktop Notifications (notify-send)",
                bool_str(self.desktop_notifications_enabled),
            ),
            ("TUI Effects & Animations", bool_str(self.animations_enabled)),
        ];

        let mut lines = Vec::with_capacity(fields.len() + 3);
        lines.push(Line::from(Span::styled(
            "Use ↑/↓ or Tab to select a setting, type or press Space to toggle, Enter to save, Esc to cancel.",
            theme().input_description,
        )));
        lines.push(Line::from(""));

        for (idx, (label, val)) in fields.into_iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let prefix = if is_selected { "▶ " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(Gruvbox::YellowBright.into())
                    .add_modifier(Modifier::BOLD)
            } else {
                theme().input_label
            };
            let val_style = if is_selected {
                theme().input_selected
            } else {
                theme().input
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{:<38}", label), label_style),
                Span::styled(format!(" {val} "), val_style),
            ]));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

fn bool_str(v: bool) -> String {
    if v {
        "[x] true".to_string()
    } else {
        "[ ] false".to_string()
    }
}
