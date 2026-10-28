use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Row, StatefulWidget, Table, TableState},
};

use crate::{
    domain::{AttentionReason, Issue},
    gruvbox::Gruvbox,
    id::IssueId,
    theme::theme,
    ui::format_relative_time,
};

#[derive(Debug, Clone)]
pub struct AttentionRow {
    pub issue: Issue,
    pub reasons: Vec<AttentionReason>,
}

#[derive(Debug, Default)]
pub struct AttentionTableState {
    pub table_state: TableState,
    pub rows: Vec<AttentionRow>,
}

impl AttentionTableState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        let idx = self.table_state.selected()?;
        self.rows.get(idx).map(|r| &r.issue)
    }

    pub fn selected_issue_id(&self) -> Option<IssueId> {
        self.selected_issue().map(|i| i.id.clone())
    }

    pub fn set_rows(&mut self, mut rows: Vec<AttentionRow>) {
        let prev_id = self.selected_issue_id();
        // View 2 is strictly sorted chronologically by `updated_at DESC`
        rows.sort_by_key(|a| std::cmp::Reverse(a.issue.updated_at));
        self.rows = rows;

        if self.rows.is_empty() {
            self.table_state.select(None);
            return;
        }

        if let Some(pid) = prev_id {
            if let Some(pos) = self.rows.iter().position(|r| r.issue.id == pid) {
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

pub struct AttentionTableWidget<'a> {
    pub focused: bool,
    pub selected_ids: &'a HashSet<IssueId>,
}

impl<'a> AttentionTableWidget<'a> {
    pub fn new(focused: bool, selected_ids: &'a HashSet<IssueId>) -> Self {
        Self {
            focused,
            selected_ids,
        }
    }
}

impl StatefulWidget for AttentionTableWidget<'_> {
    type State = AttentionTableState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.focused {
            theme().table_border_focused
        } else {
            theme().table_border
        };

        let mut title_spans = vec![
            Span::styled(" My Attention ", theme().border_title),
            Span::styled(
                format!("({} items, sorted by Updated↓) ", state.rows.len()),
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
        let mut header_cells = Vec::with_capacity(9);
        if has_sel {
            header_cells.push(Cell::from("Sel"));
        }
        header_cells.extend([
            Cell::from("ID"),
            Cell::from("Team"),
            Cell::from("Project"),
            Cell::from("Reason"),
            Cell::from("State"),
            Cell::from("Pri"),
            Cell::from("Title"),
            Cell::from("Updated"),
        ]);
        let header = Row::new(header_cells).style(header_style).height(1);

        let check_style = Style::default()
            .fg(Gruvbox::GreenBright.into())
            .add_modifier(Modifier::BOLD);

        let rows = state.rows.iter().enumerate().map(|(idx, row)| {
            let issue = &row.issue;
            let is_checked = self.selected_ids.contains(&issue.id);
            let row_bg = theme().table_row(idx);
            let proj = issue.project_name.as_deref().unwrap_or("—");

            let primary_reason = row.reasons.first().copied().unwrap_or(AttentionReason::Assigned);
            let reason_label = row
                .reasons
                .iter()
                .map(|r| r.label())
                .collect::<Vec<_>>()
                .join(", ");

            let mut row_cells = Vec::with_capacity(9);
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
                    reason_label,
                    theme().reason_style(primary_reason),
                )),
                Cell::from(Span::styled(
                    issue.state_name.as_str(),
                    theme().state_category_style(issue.state_category, Some(&issue.state_color)),
                )),
                Cell::from(Span::styled(
                    issue.priority.short_label(),
                    theme().priority_style(issue.priority),
                )),
                Cell::from(Span::styled(issue.title.as_str(), theme().issue_title)),
                Cell::from(Span::styled(
                    format_relative_time(issue.updated_at).to_string(),
                    theme().date,
                )),
            ]);

            Row::new(row_cells).style(row_bg)
        });

        let mut widths = Vec::with_capacity(9);
        if has_sel {
            widths.push(Constraint::Length(4));
        }
        widths.extend([
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Length(22),
            Constraint::Length(14),
            Constraint::Length(6),
            Constraint::Min(28),
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
