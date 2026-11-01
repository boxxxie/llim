use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::{
    domain::{Issue, Project},
    gruvbox::Gruvbox,
    theme::theme,
    ui::{
        format_relative_time, fx::window::Window, markdown::render_markdown,
        popup::utility::CenteredShrink,
    },
};

#[derive(Debug, Clone)]
pub struct ProjectDetailsPopup {
    project: Project,
    project_issues: Vec<Issue>,
    scroll: u16,
}

impl ProjectDetailsPopup {
    pub fn new(project: Project, project_issues: Vec<Issue>) -> Self {
        Self {
            project,
            project_issues,
            scroll: 0,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(76, 76)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(" Project ", theme().border.title),
            Span::styled(format!(" {} ", self.project.name), theme().border_title),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let pct = (self.project.progress * 100.0).round() as u32;
        let filled = ((pct.min(100) as usize) * 20) / 100;
        let bar = format!(
            "[{}{}] {}%",
            "█".repeat(filled),
            "░".repeat(20usize.saturating_sub(filled)),
            pct
        );

        let active_count = self.project_issues.iter().filter(|i| i.is_active()).count();
        let completed_count = self.project_issues.len().saturating_sub(active_count);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("State: ", theme().input_label),
                Span::styled(self.project.state.as_str(), theme().project_name),
                Span::raw("   "),
                Span::styled("Progress: ", theme().input_label),
                Span::styled(bar, theme().health_ok),
                Span::raw("   "),
                Span::styled("Target Date: ", theme().input_label),
                Span::styled(
                    self.project.target_date.as_deref().unwrap_or("Unset"),
                    theme().date,
                ),
            ]),
            Line::from(vec![
                Span::styled("Teams: ", theme().input_label),
                Span::styled(
                    if self.project.team_keys.is_empty() {
                        "All".to_string()
                    } else {
                        self.project.team_keys.join(", ")
                    },
                    theme().team_badge,
                ),
                Span::raw("   "),
                Span::styled("Issues: ", theme().input_label),
                Span::styled(
                    format!(
                        "{} active / {} closed ({} total)",
                        active_count,
                        completed_count,
                        self.project_issues.len()
                    ),
                    theme().time,
                ),
                Span::raw("   "),
                Span::styled("Updated: ", theme().input_label),
                Span::styled(
                    format_relative_time(self.project.updated_at).to_string(),
                    theme().date,
                ),
            ]),
            Line::from(vec![
                Span::styled("URL: ", theme().input_label),
                Span::styled(self.project.url.as_str(), theme().date),
            ]),
            Line::from(Span::styled(
                "─".repeat(inner.width as usize),
                Style::default().fg(Gruvbox::Dark3.into()),
            )),
            Line::from(Span::styled(
                "Description",
                Style::default()
                    .fg(Gruvbox::OrangeBright.into())
                    .add_modifier(Modifier::BOLD),
            )),
        ];

        if self.project.description.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "(No project description)",
                theme().input_description,
            )));
        } else {
            lines.extend(render_markdown(&self.project.description));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Active Issues in Project",
            Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
        )));

        let mut active_issues: Vec<&Issue> =
            self.project_issues.iter().filter(|i| i.is_active()).collect();
        active_issues.sort_by(|a, b| {
            b.is_epic()
                .cmp(&a.is_epic())
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });

        for issue in active_issues.into_iter().take(40) {
            let id_span = if issue.is_epic() {
                Span::styled(format!("  ◆ {:<8}", issue.identifier), theme().epic_id)
            } else {
                Span::styled(format!("  {:<10}", issue.identifier), theme().issue_id)
            };
            let mut row_spans = vec![
                id_span,
                Span::styled(
                    format!("{:<14}", issue.state_name),
                    theme().state_category_style(issue.state_category, Some(&issue.state_color)),
                ),
                Span::styled(
                    format!("{:<6}", issue.priority.short_label()),
                    theme().priority_style(issue.priority),
                ),
            ];
            if let Some(epic_badge) = issue.epic_badge_label() {
                row_spans.push(Span::styled(format!("[{epic_badge}] "), theme().epic_badge));
                row_spans.push(Span::styled(issue.clean_title(), theme().epic_title));
            } else if let Some(ref parent_ident) = issue.parent_identifier {
                row_spans.push(Span::styled(
                    format!("↳ {parent_ident} "),
                    theme().sub_issue_badge,
                ));
                row_spans.push(Span::styled(issue.title.as_str(), theme().issue_title));
            } else {
                row_spans.push(Span::styled(issue.title.as_str(), theme().issue_title));
            }
            lines.push(Line::from(row_spans));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(inner, buf);
    }
}
