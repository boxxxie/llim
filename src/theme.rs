use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};

use crate::{
    domain::{AttentionReason, IssuePriority, StateCategory},
    gruvbox::Gruvbox,
};

pub struct Theme {
    pub team_badge: Style,
    pub project_name: Style,
    pub project_description: Style,
    pub issue_id: Style,
    pub issue_title: Style,
    pub assignee: Style,
    pub label_badge: Style,
    pub date: Style,
    pub time: Style,
    pub highlight_symbol: Style,
    pub table_border: Style,
    pub table_border_focused: Style,
    pub table_row_a: Style,
    pub table_row_b: Style,
    pub background: Style,
    pub border_title: Style,
    pub active_tab: Style,
    pub inactive_tab: Style,
    pub scope_pill: Style,
    pub health_ok: Style,
    pub health_warn: Style,
    pub health_err: Style,
    pub notification: Style,
    pub notification_project: Style,
    pub input: Style,
    pub input_selected: Style,
    pub input_description: Style,
    pub input_label: Style,
    pub configuration_error: Style,
    pub activity_author: Style,
    pub activity_action: Style,
    pub border: ThemeBorder,
}

pub struct ThemeBorder {
    pub config_border: Style,
    pub title: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub fn new() -> Self {
        Self {
            team_badge: Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
            project_name: Style::default()
                .fg(Gruvbox::AquaBright.into())
                .add_modifier(Modifier::BOLD),
            project_description: Style::default()
                .fg(Gruvbox::Light4.into())
                .add_modifier(Modifier::ITALIC),
            issue_id: Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
            issue_title: Style::default().fg(Gruvbox::Light1.into()),
            assignee: Style::default().fg(Gruvbox::BlueBright.into()),
            label_badge: Style::default()
                .fg(Gruvbox::PurpleBright.into())
                .add_modifier(Modifier::ITALIC),
            date: Style::default().fg(Gruvbox::Gray244.into()),
            time: Style::default().fg(Gruvbox::Light2.into()),
            highlight_symbol: Style::default().bg(Gruvbox::Dark2.into()),
            table_border: Style::default()
                .fg(Gruvbox::Dark4.into())
                .bg(Gruvbox::Dark0.into()),
            table_border_focused: Style::default()
                .fg(Gruvbox::Orange.into())
                .bg(Gruvbox::Dark0.into()),
            table_row_a: Style::default().bg(Gruvbox::Dark0Hard.into()),
            table_row_b: Style::default().bg(Gruvbox::Dark0.into()),
            background: Style::default().bg(Gruvbox::Dark0.into()),
            border_title: Style::default()
                .fg(Gruvbox::Light2.into())
                .add_modifier(Modifier::BOLD),
            active_tab: Style::default()
                .fg(Gruvbox::Dark0Hard.into())
                .bg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
            inactive_tab: Style::default().fg(Gruvbox::Light4.into()),
            scope_pill: Style::default()
                .fg(Gruvbox::AquaBright.into())
                .add_modifier(Modifier::BOLD),
            health_ok: Style::default()
                .fg(Gruvbox::GreenBright.into())
                .add_modifier(Modifier::BOLD),
            health_warn: Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
            health_err: Style::default()
                .fg(Gruvbox::RedBright.into())
                .add_modifier(Modifier::BOLD),
            notification: Style::default()
                .bg(Gruvbox::Dark0.into())
                .fg(Gruvbox::Orange.into())
                .add_modifier(Modifier::BOLD),
            notification_project: Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
            input: Style::default()
                .fg(Gruvbox::Light2.into())
                .bg(Gruvbox::Dark0Hard.into())
                .add_modifier(Modifier::BOLD),
            input_selected: Style::default()
                .fg(Gruvbox::Light0Soft.into())
                .bg(Gruvbox::Dark0Hard.into())
                .add_modifier(Modifier::BOLD),
            input_label: Style::default()
                .fg(Gruvbox::Orange.into())
                .add_modifier(Modifier::BOLD),
            input_description: Style::default()
                .fg(Gruvbox::Gray244.into())
                .add_modifier(Modifier::ITALIC),
            configuration_error: Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
            activity_author: Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
            activity_action: Style::default().fg(Gruvbox::AquaBright.into()),
            border: ThemeBorder {
                title: Style::default()
                    .bg(Gruvbox::Orange.into())
                    .fg(Gruvbox::Dark0.into())
                    .add_modifier(Modifier::BOLD),
                config_border: Style::default()
                    .fg(Gruvbox::OrangeBright.into())
                    .bg(Gruvbox::Dark0.into()),
            },
        }
    }

    pub fn table_row(&self, idx: usize) -> Style {
        match idx % 2 {
            0 => self.table_row_a,
            _ => self.table_row_b,
        }
    }

    pub fn state_category_style(&self, category: StateCategory, hex_color: Option<&str>) -> Style {
        if let Some(hex) = hex_color {
            if let Some(color) = parse_hex_color(hex) {
                return Style::default().fg(color).add_modifier(Modifier::BOLD);
            }
        }
        match category {
            StateCategory::Started => Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
            StateCategory::Unstarted => Style::default().fg(Gruvbox::BlueBright.into()),
            StateCategory::Triage => Style::default().fg(Gruvbox::OrangeBright.into()),
            StateCategory::Backlog => Style::default().fg(Gruvbox::Gray244.into()),
            StateCategory::Completed => Style::default().fg(Gruvbox::GreenBright.into()),
            StateCategory::Canceled => Style::default().fg(Gruvbox::Dark4.into()),
        }
    }

    pub fn priority_style(&self, priority: IssuePriority) -> Style {
        match priority {
            IssuePriority::Urgent => Style::default()
                .fg(Gruvbox::RedBright.into())
                .add_modifier(Modifier::BOLD),
            IssuePriority::High => Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
            IssuePriority::Medium => Style::default().fg(Gruvbox::YellowBright.into()),
            IssuePriority::Low => Style::default().fg(Gruvbox::BlueBright.into()),
            IssuePriority::NoPriority => Style::default().fg(Gruvbox::Gray244.into()),
        }
    }

    pub fn reason_style(&self, reason: AttentionReason) -> Style {
        match reason {
            AttentionReason::Mention => Style::default()
                .fg(Gruvbox::RedBright.into())
                .add_modifier(Modifier::BOLD),
            AttentionReason::Blocked => Style::default()
                .fg(Gruvbox::RedBright.into())
                .add_modifier(Modifier::BOLD),
            AttentionReason::Urgent => Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
            AttentionReason::InProgress => Style::default().fg(Gruvbox::YellowBright.into()),
            AttentionReason::Assigned => Style::default().fg(Gruvbox::AquaBright.into()),
            AttentionReason::Todo => Style::default().fg(Gruvbox::BlueBright.into()),
        }
    }
}

pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() == 6 {
        u32::from_str_radix(trimmed, 16)
            .ok()
            .map(Color::from_u32)
    } else {
        None
    }
}

static THEME: Lazy<Theme> = Lazy::new(Theme::new);
pub(crate) fn theme() -> &'static Theme {
    &THEME
}
