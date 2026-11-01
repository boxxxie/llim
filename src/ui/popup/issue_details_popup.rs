use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};

use crate::{
    domain::{Comment, Issue, IssueHistoryEntry},
    gruvbox::Gruvbox,
    theme::theme,
    ui::{
        format_relative_time,
        markdown::{render_markdown, render_markdown_indented, render_markdown_with_options},
        render_epic_and_relations_section,
    },
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
        let same_issue = self.issue.id == issue.id;
        self.issue = issue;
        self.comments = comments;
        self.history = history;
        self.comment_draft = comment_draft;
        self.loading_activities = false;
        if !same_issue {
            self.scroll = 0;
        }
    }

    pub fn set_issue(
        &mut self,
        issue: Issue,
        comments: Vec<Comment>,
        history: Vec<IssueHistoryEntry>,
        comment_draft: Option<String>,
        loading_activities: bool,
    ) {
        if self.issue.id != issue.id {
            self.scroll = 0;
        }
        self.issue = issue;
        self.comments = comments;
        self.history = history;
        self.comment_draft = comment_draft;
        self.loading_activities = loading_activities;
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = 300;
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        Rect::new(
            terminal_area.x,
            terminal_area.y.saturating_add(1),
            terminal_area.width,
            terminal_area.height.saturating_sub(2),
        )
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        if area.width >= 72 {
            let sidebar_width = ((area.width as u32 * 30) / 100).clamp(30, 44) as u16;
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40), Constraint::Length(sidebar_width)])
                .split(area);
            self.render_main_column(cols[0], buf);
            self.render_sidebar_column(cols[1], buf);
        } else {
            self.render_main_column(area, buf);
        }
    }

    fn render_main_column(&self, area: Rect, buf: &mut Buffer) {
        let mut title_spans = vec![
            Span::styled(" ← Esc: Back ", theme().border.title),
            Span::raw(" "),
        ];
        if self.issue.is_epic() {
            title_spans.push(Span::styled(" ◆ EPIC ", theme().epic_pill));
        }
        title_spans.extend([
            Span::styled(format!(" {} ", self.issue.identifier), theme().issue_id),
            Span::styled(
                format!(" {} ", self.issue.team_name),
                theme().team_badge,
            ),
            Span::styled(
                " (j/k scroll • J/K next/prev issue) ",
                theme().date,
            ),
        ]);
        let title = Line::from(title_spans);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme().table_border_focused)
            .style(theme().background);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        let title_style = Style::default()
            .fg(Gruvbox::Light0Hard.into())
            .add_modifier(Modifier::BOLD);
        let sep_rule = "─".repeat(chunks[0].width as usize);

        let mut header_line1 = Vec::with_capacity(4);
        if let Some(epic_badge) = self.issue.epic_badge_label() {
            header_line1.push(Span::styled(
                format!("[{epic_badge}]  "),
                theme().epic_badge,
            ));
            header_line1.push(Span::styled(
                format!("{}  ", self.issue.identifier),
                theme().epic_id,
            ));
            header_line1.push(Span::styled(self.issue.clean_title(), title_style));
        } else if let Some(ref parent_ident) = self.issue.parent_identifier {
            header_line1.push(Span::styled(
                format!("{}  ", self.issue.identifier),
                theme().issue_id,
            ));
            header_line1.push(Span::styled(
                format!("[↳ {parent_ident}]  "),
                theme().sub_issue_badge,
            ));
            header_line1.push(Span::styled(self.issue.title.as_str(), title_style));
        } else {
            header_line1.push(Span::styled(
                format!("{}  ", self.issue.identifier),
                theme().issue_id,
            ));
            header_line1.push(Span::styled(self.issue.title.as_str(), title_style));
        }

        let header_lines = vec![
            Line::from(header_line1),
            Line::from(vec![
                Span::styled(
                    format!(" {} ", self.issue.state_name),
                    theme().state_category_style(
                        self.issue.state_category,
                        Some(&self.issue.state_color),
                    ),
                ),
                Span::styled("  •  ", theme().date),
                Span::styled(
                    self.issue.priority.label(),
                    theme().priority_style(self.issue.priority),
                ),
                Span::styled("  •  Updated ", theme().date),
                Span::styled(
                    format_relative_time(self.issue.updated_at).to_string(),
                    theme().time,
                ),
            ]),
            Line::from(Span::styled(
                sep_rule.clone(),
                Style::default().fg(Gruvbox::Dark3.into()),
            )),
        ];
        Paragraph::new(header_lines)
            .wrap(Wrap { trim: false })
            .render(chunks[0], buf);

        let section_heading = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let subtle_divider = Style::default().fg(Gruvbox::Dark2.into());

        let mut body_lines: Vec<Line> = Vec::new();
        let epic_lines = render_epic_and_relations_section(&self.issue, "");
        if !epic_lines.is_empty() {
            body_lines.extend(epic_lines);
            body_lines.push(Line::from(Span::styled(sep_rule.clone(), subtle_divider)));
        }
        body_lines.push(Line::from(Span::styled("▍ Description", section_heading)));

        match self.issue.description.as_deref() {
            Some(desc) if !desc.trim().is_empty() => {
                body_lines.extend(render_markdown(desc));
            }
            _ => {
                body_lines.push(Line::from(Span::styled(
                    "(No description — press 'e' to add one)",
                    theme().input_description,
                )));
            }
        }

        body_lines.push(Line::from(""));
        body_lines.push(Line::from(Span::styled(sep_rule.clone(), subtle_divider)));

        let mut comment_header_spans = vec![Span::styled(
            format!("▍ Comments ({})", self.comments.len()),
            section_heading,
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
                Span::styled("╭─ ✎ [Draft] ", theme().health_warn),
                Span::styled(
                    "(unsent local draft — press 'c' to edit/send, Ctrl+x in editor to delete)",
                    theme().input_description,
                ),
            ]));
            body_lines.extend(render_markdown_with_options(
                draft,
                "│ ",
                theme().project_description,
            ));
            body_lines.push(Line::from(""));
        }

        if self.comments.is_empty() && self.comment_draft.is_none() {
            let msg = if self.loading_activities {
                "Loading comments from Linear..."
            } else {
                "No comments on this issue (press 'c' to post a comment)."
            };
            body_lines.push(Line::from(Span::styled(msg, theme().input_description)));
        } else {
            for comment in &self.comments {
                body_lines.push(Line::from(vec![
                    Span::styled(
                        "╭─ ",
                        Style::default().fg(Gruvbox::Dark3.into()),
                    ),
                    Span::styled(
                        format!("{} ", comment.user_name),
                        theme().activity_author,
                    ),
                    Span::styled(
                        format!(
                            "• {} ({})",
                            format_relative_time(comment.created_at),
                            comment.created_at.format("%Y-%m-%d %H:%M UTC")
                        ),
                        theme().date,
                    ),
                ]));
                body_lines.extend(render_markdown_indented(&comment.body, "│ "));
                body_lines.push(Line::from(""));
            }
        }

        body_lines.push(Line::from(Span::styled(sep_rule, subtle_divider)));
        body_lines.push(Line::from(Span::styled(
            format!("▍ Timeline & Changes ({})", self.history.len()),
            section_heading,
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
                        format!("  • {:<8} ", format_relative_time(entry.created_at)),
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

    fn render_sidebar_column(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(" Properties & Actions ", theme().border_title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme().table_border)
            .style(theme().background);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 4 || inner.width < 12 {
            return;
        }

        let section_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let key_badge = Style::default()
            .fg(Gruvbox::YellowBright.into())
            .add_modifier(Modifier::BOLD);
        let divider_line = Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::default().fg(Gruvbox::Dark3.into()),
        ));

        let (assignee_str, is_creator_fallback) = self.issue.assignee_or_creator_display();
        let assignee_style = if is_creator_fallback {
            theme().assignee.add_modifier(Modifier::ITALIC)
        } else {
            theme().assignee
        };
        let project_str = self
            .issue
            .project_name
            .as_deref()
            .unwrap_or("Unassigned");
        let branch_str = self
            .issue
            .git_branch_name
            .as_deref()
            .unwrap_or("(press 'b' to generate)");

        let mut lines: Vec<Line> = Vec::with_capacity(32);

        lines.push(Line::from(Span::styled("▍ Issue Properties", section_style)));
        if let Some(epic_badge) = self.issue.epic_badge_label() {
            lines.push(Line::from(vec![
                Span::styled("Type         ", theme().input_label),
                Span::styled(epic_badge.to_string(), theme().epic_badge),
            ]));
        }
        if let Some(ref parent_ident) = self.issue.parent_identifier {
            lines.push(Line::from(vec![
                Span::styled("Parent Epic  ", theme().input_label),
                Span::styled(format!("◆ {parent_ident}"), theme().sub_issue_badge),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("ID       ", theme().input_label),
            Span::styled("[y] ", key_badge),
            Span::styled(self.issue.identifier.as_str(), theme().issue_id),
        ]));
        lines.push(Line::from(vec![
            Span::styled("State    ", theme().input_label),
            Span::styled("[x] ", key_badge),
            Span::styled(
                self.issue.state_name.as_str(),
                theme().state_category_style(
                    self.issue.state_category,
                    Some(&self.issue.state_color),
                ),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Priority ", theme().input_label),
            Span::styled("[!] ", key_badge),
            Span::styled(
                self.issue.priority.label(),
                theme().priority_style(self.issue.priority),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Assignee ", theme().input_label),
            Span::styled("[A] ", key_badge),
            Span::styled(assignee_str, assignee_style),
        ]));
        if let Some(ref creator) = self.issue.creator_name {
            lines.push(Line::from(vec![
                Span::styled("Creator      ", theme().input_label),
                Span::styled(creator.as_str(), theme().time),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Team         ", theme().input_label),
            Span::styled(
                format!("{} ({})", self.issue.team_name, self.issue.team_key),
                theme().team_badge,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Project  ", theme().input_label),
            Span::styled("[M] ", key_badge),
            Span::styled(project_str, theme().project_name),
        ]));

        if self.issue.labels.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Labels   ", theme().input_label),
                Span::styled("[l] ", key_badge),
                Span::styled("None", theme().input_description),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("Labels   ", theme().input_label),
                Span::styled("[l] ", key_badge),
                Span::styled(self.issue.labels.join(", "), theme().label_badge),
            ]));
        }

        if !self.issue.blocked_by.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Blocked      ", theme().health_err),
                Span::styled(self.issue.blocked_by.join(", "), theme().health_err),
            ]));
        }

        lines.push(divider_line.clone());
        lines.push(Line::from(Span::styled("▍ Git & Links", section_style)));
        lines.push(Line::from(vec![
            Span::styled("Branch   ", theme().input_label),
            Span::styled("[b] ", key_badge),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  {branch_str}"),
            theme().time,
        )));
        lines.push(Line::from(vec![
            Span::styled("Browser  ", theme().input_label),
            Span::styled("[w] ", key_badge),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  {}", self.issue.url),
            theme().date,
        )));

        lines.push(divider_line.clone());
        lines.push(Line::from(Span::styled("▍ Timestamps", section_style)));
        lines.push(Line::from(vec![
            Span::styled("Updated  ", theme().input_label),
            Span::styled(
                format!(
                    "{} ({})",
                    format_relative_time(self.issue.updated_at),
                    self.issue.updated_at.format("%Y-%m-%d")
                ),
                theme().date,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Created  ", theme().input_label),
            Span::styled(
                format!(
                    "{} ({})",
                    format_relative_time(self.issue.created_at),
                    self.issue.created_at.format("%Y-%m-%d")
                ),
                theme().date,
            ),
        ]));
        if let Some(done_at) = self.issue.completed_at {
            lines.push(Line::from(vec![
                Span::styled("Done     ", theme().input_label),
                Span::styled(
                    format!(
                        "{} ({})",
                        format_relative_time(done_at),
                        done_at.format("%Y-%m-%d")
                    ),
                    theme().health_ok,
                ),
            ]));
        }

        lines.push(divider_line);
        lines.push(Line::from(Span::styled("▍ Quick Actions", section_style)));
        let actions = [
            ("[c]", "Post / edit comment"),
            ("[e]", "Edit issue details"),
            ("[m]", "Assign to yourself"),
            ("[J/K]", "Next / prev issue"),
            ("[Esc]", "Back to issue list"),
        ];
        for (k, desc) in actions {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<6}", k), key_badge),
                Span::styled(desc, theme().issue_title),
            ]));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}
