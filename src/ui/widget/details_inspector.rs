use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};

use crate::{
    domain::{ActivityEvent, Comment, Issue, IssueHistoryEntry},
    gruvbox::Gruvbox,
    id::{ActivityId, IssueId},
    theme::theme,
    ui::{
        format_relative_time,
        markdown::{render_markdown, render_markdown_indented, render_markdown_with_options},
        render_epic_and_relations_section,
    },
};

#[derive(Debug, Default, Clone)]
pub struct DetailsInspectorState {
    pub issue: Option<Issue>,
    pub selected_activity: Option<ActivityEvent>,
    pub comments: Vec<Comment>,
    pub history: Vec<IssueHistoryEntry>,
    pub comment_draft: Option<String>,
    pub scroll: u16,
    pub loading_activities: bool,
}

impl DetailsInspectorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_issue_id(&self) -> Option<&IssueId> {
        self.issue.as_ref().map(|i| &i.id)
    }

    pub fn current_activity_id(&self) -> Option<&ActivityId> {
        self.selected_activity.as_ref().map(|a| &a.id)
    }

    pub fn update_selection(
        &mut self,
        issue: Option<Issue>,
        selected_activity: Option<ActivityEvent>,
        comments: Vec<Comment>,
        history: Vec<IssueHistoryEntry>,
        comment_draft: Option<String>,
        loading_activities: bool,
    ) {
        let same_issue = self.issue.as_ref().map(|i| &i.id) == issue.as_ref().map(|i| &i.id);
        let same_activity =
            self.selected_activity.as_ref().map(|a| &a.id) == selected_activity.as_ref().map(|a| &a.id);

        if !same_issue || !same_activity {
            self.scroll = 0;
        }

        self.issue = issue;
        self.selected_activity = selected_activity;
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
        self.scroll = 200;
    }
}

pub struct DetailsInspectorWidget {
    pub focused: bool,
}

impl DetailsInspectorWidget {
    pub fn new(focused: bool) -> Self {
        Self { focused }
    }
}

impl StatefulWidget for DetailsInspectorWidget {
    type State = DetailsInspectorState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.focused {
            theme().table_border_focused
        } else {
            theme().table_border
        };

        let title = match (&state.issue, &state.selected_activity) {
            (Some(issue), _) => {
                let mut spans = Vec::with_capacity(5);
                if issue.is_epic() {
                    spans.push(Span::styled(" ◆ EPIC ", theme().epic_pill));
                }
                spans.push(Span::styled(
                    format!(" {} ", issue.identifier),
                    theme().border.title,
                ));
                if let Some(ref parent_ident) = issue.parent_identifier {
                    spans.push(Span::styled(
                        format!(" ↳ {parent_ident} "),
                        theme().sub_issue_badge,
                    ));
                }
                spans.push(Span::styled(
                    format!(" {} ", issue.clean_title()),
                    theme().border_title,
                ));
                spans.push(Span::styled(
                    "(←/→ or J/K to scroll, c to comment, a to hide) ",
                    theme().date,
                ));
                Line::from(spans)
            }
            (None, Some(act)) => Line::from(vec![
                Span::styled(
                    format!(" {} ", act.issue_identifier),
                    theme().border.title,
                ),
                Span::styled(format!(" {} ", act.issue_title), theme().border_title),
                Span::styled(
                    "(←/→ or J/K to scroll, c to comment, a to hide) ",
                    theme().date,
                ),
            ]),
            (None, None) => Line::from(vec![
                Span::styled(" Details Inspector ", theme().border_title),
                Span::styled("(No item selected) ", theme().date),
            ]),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(theme().background);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        let Some(ref issue) = state.issue else {
            if let Some(ref act) = state.selected_activity {
                let fallback_lines = vec![
                    Line::from(vec![
                        Span::styled("Actor: ", theme().input_label),
                        Span::styled(act.actor_name.as_str(), theme().activity_author),
                        Span::raw("   "),
                        Span::styled("When: ", theme().input_label),
                        Span::styled(
                            format_relative_time(act.timestamp).to_string(),
                            theme().date,
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Change: ", theme().input_label),
                        Span::styled(act.summary.as_str(), theme().activity_action),
                    ]),
                ];
                Paragraph::new(fallback_lines)
                    .wrap(Wrap { trim: false })
                    .scroll((state.scroll, 0))
                    .render(inner, buf);
            } else {
                Paragraph::new(Line::from(Span::styled(
                    "Select an issue or activity event in the top pane to inspect its details.",
                    theme().input_description,
                )))
                .render(inner, buf);
            }
            return;
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(1)])
            .split(inner);

        let branch = issue
            .git_branch_name
            .as_deref()
            .unwrap_or("(press 'b' to copy generated branch)");
        let project_str = issue.project_name.as_deref().unwrap_or("Unassigned");
        let (assignee_str, is_creator_fallback) = issue.assignee_or_creator_display();
        let assignee_style = if is_creator_fallback {
            theme().assignee.add_modifier(Modifier::ITALIC)
        } else {
            theme().assignee
        };
        let labels_str = if issue.labels.is_empty() {
            "None".to_string()
        } else {
            issue.labels.join(", ")
        };

        let meta_lines = vec![
            Line::from(vec![
                Span::styled("Team: ", theme().input_label),
                Span::styled(
                    format!("{} ({})", issue.team_name, issue.team_key),
                    theme().team_badge,
                ),
                Span::raw("   "),
                Span::styled("Project: ", theme().input_label),
                Span::styled(project_str, theme().project_name),
                Span::raw("   "),
                Span::styled("State: ", theme().input_label),
                Span::styled(
                    issue.state_name.as_str(),
                    theme().state_category_style(issue.state_category, Some(&issue.state_color)),
                ),
                Span::raw("   "),
                Span::styled("Priority: ", theme().input_label),
                Span::styled(
                    issue.priority.label(),
                    theme().priority_style(issue.priority),
                ),
            ]),
            Line::from(vec![
                Span::styled("Assignee: ", theme().input_label),
                Span::styled(assignee_str, assignee_style),
                Span::raw("   "),
                Span::styled("Labels: ", theme().input_label),
                Span::styled(labels_str, theme().label_badge),
                Span::raw("   "),
                Span::styled("Updated: ", theme().input_label),
                Span::styled(
                    format_relative_time(issue.updated_at).to_string(),
                    theme().date,
                ),
                Span::raw("   "),
                Span::styled("Created: ", theme().input_label),
                Span::styled(
                    format_relative_time(issue.created_at).to_string(),
                    theme().date,
                ),
                if !issue.blocked_by.is_empty() {
                    Span::styled(
                        format!("   Blocked by: {}", issue.blocked_by.join(", ")),
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
                Span::styled(issue.url.as_str(), theme().date),
            ]),
            Line::from(Span::styled(
                "─".repeat(chunks[0].width as usize),
                Style::default().fg(Gruvbox::Dark3.into()),
            )),
        ];
        Paragraph::new(meta_lines).render(chunks[0], buf);

        let heading_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);

        let mut body_lines: Vec<Line> = Vec::new();

        if let Some(ref act) = state.selected_activity {
            body_lines.push(Line::from(vec![
                Span::styled("Selected Activity: ", heading_style),
                Span::styled(format!("{} ", act.actor_name), theme().activity_author),
                Span::styled(
                    format!("({}) — ", format_relative_time(act.timestamp)),
                    theme().date,
                ),
                Span::styled(act.summary.as_str(), theme().activity_action),
            ]));
            if let Some(ref act_body) = act.body {
                if !act_body.trim().is_empty() && Some(act_body.as_str()) != issue.description.as_deref() {
                    body_lines.extend(render_markdown_indented(act_body, "  "));
                }
            }
            body_lines.push(Line::from(""));
        }

        body_lines.extend(render_epic_and_relations_section(issue, ""));

        body_lines.push(Line::from(Span::styled("Description", heading_style)));
        match issue.description.as_deref() {
            Some(desc) if !desc.trim().is_empty() => {
                body_lines.extend(render_markdown(desc));
            }
            _ => {
                body_lines.push(Line::from(Span::styled(
                    "(No description)",
                    theme().input_description,
                )));
            }
        }

        body_lines.push(Line::from(""));
        let mut comment_header_spans = vec![Span::styled(
            format!("Comments ({})", state.comments.len()),
            heading_style,
        )];
        if state.comment_draft.is_some() {
            comment_header_spans.push(Span::styled(
                "  [DRAFT — press 'c' to resume or discard]",
                theme().health_warn,
            ));
        }
        body_lines.push(Line::from(comment_header_spans));

        if let Some(ref draft) = state.comment_draft {
            body_lines.push(Line::from(vec![
                Span::styled("• [Draft] ", theme().health_warn),
                Span::styled(
                    "(unsent local draft — press 'c' to edit/send, Ctrl+x in editor to delete)",
                    theme().input_description,
                ),
            ]));
            body_lines.extend(render_markdown_with_options(
                draft,
                "  ",
                theme().project_description,
            ));
            body_lines.push(Line::from(""));
        }

        if state.comments.is_empty() && state.comment_draft.is_none() {
            let msg = if state.loading_activities {
                "Loading comments from Linear..."
            } else {
                "No comments on this issue."
            };
            body_lines.push(Line::from(Span::styled(msg, theme().input_description)));
        } else {
            for comment in &state.comments {
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
                body_lines.extend(render_markdown_indented(&comment.body, "  "));
                body_lines.push(Line::from(""));
            }
        }

        body_lines.push(Line::from(Span::styled(
            format!("Timeline & Changes ({})", state.history.len()),
            heading_style,
        )));

        if state.history.is_empty() {
            let msg = if state.loading_activities {
                "Loading change history from Linear..."
            } else {
                "No recorded state/field transitions."
            };
            body_lines.push(Line::from(Span::styled(msg, theme().input_description)));
        } else {
            for entry in state.history.iter().rev() {
                let actor = entry.actor_name.as_deref().unwrap_or("Linear");
                body_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", format_relative_time(entry.created_at)),
                        theme().date,
                    ),
                    Span::styled(format!("{actor}: "), theme().activity_author),
                    Span::styled(entry.summary().to_string(), theme().activity_action),
                ]));
            }
        }

        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0))
            .render(chunks[1], buf);
    }
}
