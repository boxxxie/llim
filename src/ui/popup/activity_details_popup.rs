use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::{
    domain::ActivityEvent,
    gruvbox::Gruvbox,
    theme::theme,
    ui::{
        format_relative_time, fx::window::Window, markdown::render_markdown,
        popup::utility::CenteredShrink,
    },
};

#[derive(Debug, Clone)]
pub struct ActivityDetailsPopup {
    activity: ActivityEvent,
    scroll: u16,
}

impl ActivityDetailsPopup {
    pub fn new(activity: ActivityEvent) -> Self {
        Self {
            activity,
            scroll: 0,
        }
    }

    pub fn activity(&self) -> &ActivityEvent {
        &self.activity
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(72, 65)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(" Activity ", theme().border.title),
            Span::styled(
                format!(
                    " [{}] {} — {} ",
                    self.activity.team_key, self.activity.issue_identifier, self.activity.issue_title
                ),
                theme().border_title,
            ),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Actor: ", theme().input_label),
                Span::styled(self.activity.actor_name.as_str(), theme().activity_author),
                Span::raw("   "),
                Span::styled("When: ", theme().input_label),
                Span::styled(
                    format!(
                        "{} ({})",
                        format_relative_time(self.activity.timestamp),
                        self.activity.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                    ),
                    theme().date,
                ),
            ]),
            Line::from(vec![
                Span::styled("Project: ", theme().input_label),
                Span::styled(
                    self.activity
                        .project_name
                        .as_deref()
                        .unwrap_or("Unassigned"),
                    theme().project_name,
                ),
                Span::raw("   "),
                Span::styled("Summary: ", theme().input_label),
                Span::styled(self.activity.summary.as_str(), theme().activity_action),
            ]),
            Line::from(vec![
                Span::styled("URL: ", theme().input_label),
                Span::styled(self.activity.url.as_str(), theme().date),
            ]),
            Line::from(Span::styled(
                "─".repeat(inner.width as usize),
                Style::default().fg(Gruvbox::Dark3.into()),
            )),
            Line::from(Span::styled(
                "Details / Body (Press Enter to open full Issue popup)",
                Style::default()
                    .fg(Gruvbox::OrangeBright.into())
                    .add_modifier(Modifier::BOLD),
            )),
        ];

        if let Some(ref body) = self.activity.body {
            if !body.trim().is_empty() {
                lines.extend(render_markdown(body));
            } else {
                lines.push(Line::from(Span::styled(
                    self.activity.summary.to_string(),
                    theme().issue_title,
                )));
            }
        } else {
            lines.push(Line::from(Span::styled(
                self.activity.summary.to_string(),
                theme().issue_title,
            )));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(inner, buf);
    }
}
