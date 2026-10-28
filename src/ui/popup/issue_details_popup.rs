use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::{
    domain::{Comment, Issue, IssueHistoryEntry},
    gruvbox::Gruvbox,
    theme::theme,
    ui::{format_relative_time, fx::window::Window, popup::utility::CenteredShrink},
};

#[derive(Debug, Clone)]
pub struct IssueDetailsPopup {
    issue: Issue,
    comments: Vec<Comment>,
    history: Vec<IssueHistoryEntry>,
    comment_draft: Option<String>,
    scroll: u16,
    loading_activities: bool,
}

impl IssueDetailsPopup {
    pub fn new(
        issue: Issue,
        comments: Vec<Comment>,
        history: Vec<IssueHistoryEntry>,
        comment_draft: Option<String>,
        loading_activities: bool,
    ) -> Self {
        Self {
            issue,
            comments,
            history,
            comment_draft,
            scroll: 0,
            loading_activities,
        }
    }

    pub fn issue(&self) -> &Issue {
        &self.issue
    }

    pub fn update_data(
        &mut self,
        issue: Issue,
        comments: Vec<Comment>,
        history: Vec<IssueHistoryEntry>,
        comment_draft: Option<String>,
    ) {
        self.issue = issue;
        self.comments = comments;
        self.history = history;
        self.comment_draft = comment_draft;
        self.loading_activities = false;
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(84, 84)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(
                format!(" {} ", self.issue.identifier),
                theme().border.title,
            ),
            Span::styled(
                format!(" {} ", self.issue.title),
                theme().border_title,
            ),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        if inner.height < 4 || inner.width < 10 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(inner);

        let branch = self
            .issue
            .git_branch_name
            .as_deref()
            .unwrap_or("(press 'b' to generate)");
        let project_str = self
            .issue
            .project_name
            .as_deref()
            .unwrap_or("Unassigned");
        let (assignee_str, is_creator_fallback) = self.issue.assignee_or_creator_display();
        let assignee_style = if is_creator_fallback {
            theme().assignee.add_modifier(Modifier::ITALIC)
        } else {
            theme().assignee
        };
        let labels_str = if self.issue.labels.is_empty() {
            "None".to_string()
        } else {
            self.issue.labels.join(", ")
        };

        let meta_lines = vec![
            Line::from(vec![
                Span::styled("Team: ", theme().input_label),
                Span::styled(
                    format!("{} ({})", self.issue.team_name, self.issue.team_key),
                    theme().team_badge,
                ),
                Span::raw("   "),
                Span::styled("Project: ", theme().input_label),
                Span::styled(project_str, theme().project_name),
                Span::raw("   "),
                Span::styled("Updated: ", theme().input_label),
                Span::styled(
                    format_relative_time(self.issue.updated_at).to_string(),
                    theme().date,
                ),
            ]),
            Line::from(vec![
                Span::styled("State: ", theme().input_label),
                Span::styled(
                    self.issue.state_name.as_str(),
                    theme().state_category_style(
                        self.issue.state_category,
                        Some(&self.issue.state_color),
                    ),
                ),
                Span::raw("   "),
                Span::styled("Priority: ", theme().input_label),
                Span::styled(
                    self.issue.priority.label(),
                    theme().priority_style(self.issue.priority),
                ),
                Span::raw("   "),
                Span::styled("Assignee: ", theme().input_label),
                Span::styled(assignee_str, assignee_style),
            ]),
            Line::from(vec![
                Span::styled("Labels: ", theme().input_label),
                Span::styled(labels_str, theme().label_badge),
                if !self.issue.blocked_by.is_empty() {
                    Span::styled(
                        format!("   Blocked by: {}", self.issue.blocked_by.join(", ")),
                        theme().health_err,
                    )
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(vec![
                Span::styled("Branch: ", theme().input_label),
                Span::styled(branch, theme().time),
                Span::raw("   "),
                Span::styled("URL: ", theme().input_label),
                Span::styled(self.issue.url.as_str(), theme().date),
            ]),
            Line::from(Span::styled(
                "─".repeat(chunks[0].width as usize),
                Style::default().fg(Gruvbox::Dark3.into()),
            )),
        ];
        Paragraph::new(meta_lines).render(chunks[0], buf);

        let mut body_lines: Vec<Line> = Vec::new();
        body_lines.push(Line::from(Span::styled(
            "Description",
            Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
        )));
        match self.issue.description.as_deref() {
            Some(desc) if !desc.trim().is_empty() => {
                for l in desc.lines() {
                    body_lines.push(Line::from(Span::styled(
                        l.to_string(),
                        theme().issue_title,
                    )));
                }
            }
            _ => {
                body_lines.push(Line::from(Span::styled(
                    "(No description)",
                    theme().input_description,
                )));
            }
        }

        body_lines.push(Line::from(""));
        let heading = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let mut comment_header_spans = vec![Span::styled(
            format!("Comments ({})", self.comments.len()),
            heading,
        )];
        if self.comment_draft.is_some() {
            comment_header_spans.push(Span::styled(
                "  [DRAFT — press 'c' to resume or discard]",
                theme().health_warn,
            ));
        }
        body_lines.push(Line::from(comment_header_spans));

        if let Some(ref draft) = self.comment_draft {
            body_lines.push(Line::from(vec![
                Span::styled("• [Draft] ", theme().health_warn),
                Span::styled(
                    "(unsent local draft — press 'c' to edit/send, Ctrl+x in editor to delete)",
                    theme().input_description,
                ),
            ]));
            for line in draft.lines() {
                body_lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    theme().project_description,
                )));
            }
            body_lines.push(Line::from(""));
        }

        if self.comments.is_empty() && self.comment_draft.is_none() {
            let msg = if self.loading_activities {
                "Loading comments from Linear..."
            } else {
                "No comments on this issue."
            };
            body_lines.push(Line::from(Span::styled(msg, theme().input_description)));
        } else {
            for comment in &self.comments {
                body_lines.push(Line::from(vec![
                    Span::styled(
                        format!("• {} ", comment.user_name),
                        theme().activity_author,
                    ),
                    Span::styled(
                        format!("({})", format_relative_time(comment.created_at)),
                        theme().date,
                    ),
                ]));
                for line in comment.body.lines() {
                    body_lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        theme().issue_title,
                    )));
                }
                body_lines.push(Line::from(""));
            }
        }

        body_lines.push(Line::from(Span::styled(
            format!("Timeline & Changes ({})", self.history.len()),
            Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD),
        )));

        if self.history.is_empty() {
            let msg = if self.loading_activities {
                "Loading change history from Linear..."
            } else {
                "No recorded state/field transitions."
            };
            body_lines.push(Line::from(Span::styled(msg, theme().input_description)));
        } else {
            for entry in self.history.iter().rev() {
                let actor = entry.actor_name.as_deref().unwrap_or("Linear");
                body_lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "  {} ",
                            format_relative_time(entry.created_at)
                        ),
                        theme().date,
                    ),
                    Span::styled(format!("{actor}: "), theme().activity_author),
                    Span::styled(entry.summary().to_string(), theme().activity_action),
                ]));
            }
        }

        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(chunks[1], buf);
    }
}
