use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Row, StatefulWidget, Table, TableState},
};

use crate::{
    domain::{Issue, IssueSortOrder},
    gruvbox::Gruvbox,
    id::IssueId,
    theme::theme,
    ui::format_relative_time,
};

#[derive(Debug, Default)]
pub struct IssuesTableState {
    pub table_state: TableState,
    pub rows: Vec<Issue>,
    pub sort_order: IssueSortOrder,
    pub show_completed: bool,
}

impl IssuesTableState {
    pub fn new(show_completed: bool) -> Self {
        Self {
            table_state: TableState::default(),
            rows: Vec::new(),
            sort_order: IssueSortOrder::default(),
            show_completed,
        }
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        let idx = self.table_state.selected()?;
        self.rows.get(idx)
    }

    pub fn selected_issue_id(&self) -> Option<IssueId> {
        self.selected_issue().map(|i| i.id.clone())
    }

    pub fn set_rows(&mut self, mut rows: Vec<Issue>) {
        let prev_id = self.selected_issue_id();
        self.sort_rows(&mut rows);
        self.rows = rows;

        if self.rows.is_empty() {
            self.table_state.select(None);
            return;
        }

        if let Some(pid) = prev_id {
            if let Some(pos) = self.rows.iter().position(|r| r.id == pid) {
                self.table_state.select(Some(pos));
                return;
            }
        }

        let clamped = self
            .table_state
            .selected()
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
        self.table_state.select(Some(clamped));
    }

    pub fn cycle_sort(&mut self) {
        self.sort_order = self.sort_order.next();
        let mut rows = std::mem::take(&mut self.rows);
        self.sort_rows(&mut rows);
        self.set_rows(rows);
    }

    fn sort_rows(&self, rows: &mut [Issue]) {
        match self.sort_order {
            IssueSortOrder::PriorityAndUpdated => {
                rows.sort_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then_with(|| b.updated_at.cmp(&a.updated_at))
                });
            }
            IssueSortOrder::UpdatedDesc => {
                rows.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
            }
            IssueSortOrder::CreatedDesc => {
                rows.sort_by_key(|a| std::cmp::Reverse(a.created_at));
            }
            IssueSortOrder::StateAndPriority => {
                rows.sort_by(|a, b| {
                    a.state_category
                        .cmp(&b.state_category)
                        .then_with(|| a.priority.cmp(&b.priority))
                        .then_with(|| b.updated_at.cmp(&a.updated_at))
                });
            }
            IssueSortOrder::Identifier => {
                rows.sort_by(|a, b| a.identifier.cmp(&b.identifier));
            }
        }
    }

    pub fn next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) => (i + 1).min(self.rows.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn previous(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    pub fn page_down(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) => (i + 10).min(self.rows.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn page_up(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(10),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    pub fn first(&mut self) {
        if !self.rows.is_empty() {
            self.table_state.select(Some(0));
        }
    }

    pub fn last(&mut self) {
        if !self.rows.is_empty() {
            self.table_state.select(Some(self.rows.len() - 1));
        }
    }
}

pub struct IssuesTableWidget<'a> {
    pub focused: bool,
    pub selected_ids: &'a HashSet<IssueId>,
}

impl<'a> IssuesTableWidget<'a> {
    pub fn new(focused: bool, selected_ids: &'a HashSet<IssueId>) -> Self {
        Self {
            focused,
            selected_ids,
        }
    }
}

impl StatefulWidget for IssuesTableWidget<'_> {
    type State = IssuesTableState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.focused {
            theme().table_border_focused
        } else {
            theme().table_border
        };

        let mut title_spans = vec![
            Span::styled(" Issues ", theme().border_title),
            Span::styled(
                format!(
                    "({} shown, sort: {}{}) ",
                    state.rows.len(),
                    state.sort_order.label(),
                    if state.show_completed {
                        ", +closed"
                    } else {
                        ""
                    }
                ),
                theme().date,
            ),
        ];
        if !self.selected_ids.is_empty() {
            title_spans.push(Span::styled(
                format!(" [✓ {} selected] ", self.selected_ids.len()),
                theme().active_tab,
            ));
        }
        let title = Line::from(title_spans);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(theme().background);

        let has_sel = !self.selected_ids.is_empty();
        let header_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let mut header_cells = Vec::with_capacity(10);
        if has_sel {
            header_cells.push(Cell::from("Sel"));
        }
        header_cells.extend([
            Cell::from("ID"),
            Cell::from("Team"),
            Cell::from("Project"),
            Cell::from("State"),
            Cell::from("Pri"),
            Cell::from("Title"),
            Cell::from("Assignee"),
            Cell::from("Labels"),
            Cell::from("Updated"),
        ]);
        let header = Row::new(header_cells).style(header_style).height(1);

        let check_style = Style::default()
            .fg(Gruvbox::GreenBright.into())
            .add_modifier(Modifier::BOLD);

        let rows = state.rows.iter().enumerate().map(|(idx, issue)| {
            let is_checked = self.selected_ids.contains(&issue.id);
            let row_bg = theme().table_row(idx);
            let proj = issue.project_name.as_deref().unwrap_or("—");
            let (assignee, is_creator_fallback) = issue.assignee_or_creator_display();
            let assignee_style = if is_creator_fallback {
                theme().assignee.add_modifier(Modifier::ITALIC)
            } else {
                theme().assignee
            };
            let labels = if issue.labels.is_empty() {
                "—".to_string()
            } else {
                issue.labels.join(", ")
            };
            let title_prefix = if issue.has_unacknowledged_mention {
                "[@] "
            } else if issue.is_blocked() {
                "[BLK] "
            } else {
                ""
            };

            let mut row_cells = Vec::with_capacity(10);
            if has_sel {
                if is_checked {
                    row_cells.push(Cell::from(Span::styled("[✓]", check_style)));
                } else {
                    row_cells.push(Cell::from(Span::styled("[ ]", theme().date)));
                }
            }
            row_cells.extend([
                Cell::from(Span::styled(issue.identifier.as_str(), theme().issue_id)),
                Cell::from(Span::styled(issue.team_key.as_str(), theme().team_badge)),
                Cell::from(Span::styled(proj, theme().project_name)),
                Cell::from(Span::styled(
                    issue.state_name.as_str(),
                    theme().state_category_style(issue.state_category, Some(&issue.state_color)),
                )),
                Cell::from(Span::styled(
                    issue.priority.short_label(),
                    theme().priority_style(issue.priority),
                )),
                Cell::from(Line::from(vec![
                    if !title_prefix.is_empty() {
                        Span::styled(title_prefix, theme().health_err)
                    } else {
                        Span::raw("")
                    },
                    Span::styled(issue.title.as_str(), theme().issue_title),
                ])),
                Cell::from(Span::styled(assignee, assignee_style)),
                Cell::from(Span::styled(labels, theme().label_badge)),
                Cell::from(Span::styled(
                    format_relative_time(issue.updated_at).to_string(),
                    theme().date,
                )),
            ]);

            Row::new(row_cells).style(row_bg)
        });

        let mut widths = Vec::with_capacity(10);
        if has_sel {
            widths.push(Constraint::Length(4));
        }
        widths.extend([
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Length(14),
            Constraint::Length(6),
            Constraint::Min(28),
            Constraint::Length(18),
            Constraint::Length(16),
            Constraint::Length(10),
        ]);

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(theme().highlight_symbol)
            .highlight_symbol("▶ ");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}
