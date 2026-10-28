use std::collections::HashSet;

use compact_str::CompactString;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Row, StatefulWidget, Table, TableState},
};

use crate::{
    domain::ActivityEvent,
    gruvbox::Gruvbox,
    id::{ActivityId, IssueId},
    theme::theme,
    ui::format_relative_time,
};

#[derive(Debug, Clone)]
pub enum FeedRowItem {
    Single(ActivityEvent),
    BatchSummary {
        batch_id: u64,
        team_key: CompactString,
        actor_name: CompactString,
        count: usize,
        expanded: bool,
        first_event: ActivityEvent,
    },
}

impl FeedRowItem {
    pub fn primary_event(&self) -> &ActivityEvent {
        match self {
            Self::Single(ev) => ev,
            Self::BatchSummary { first_event, .. } => first_event,
        }
    }

    pub fn issue_id(&self) -> &IssueId {
        &self.primary_event().issue_id
    }

    pub fn activity_id(&self) -> &ActivityId {
        &self.primary_event().id
    }
}

#[derive(Debug, Default)]
pub struct ActivityFeedState {
    pub table_state: TableState,
    pub raw_events: Vec<ActivityEvent>,
    pub display_rows: Vec<FeedRowItem>,
    pub expanded_batches: HashSet<u64>,
}

impl ActivityFeedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_row(&self) -> Option<&FeedRowItem> {
        let idx = self.table_state.selected()?;
        self.display_rows.get(idx)
    }

    pub fn selected_event(&self) -> Option<&ActivityEvent> {
        self.selected_row().map(|r| r.primary_event())
    }

    pub fn set_events(&mut self, events: Vec<ActivityEvent>) {
        self.raw_events = events;
        self.rebuild_display_rows();
    }

    pub fn toggle_selected_batch(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let batch_id = match row {
            FeedRowItem::BatchSummary { batch_id, .. } => *batch_id,
            FeedRowItem::Single(ev) => ev.batch_id,
        };
        if batch_id == 0 {
            return;
        }
        if !self.expanded_batches.remove(&batch_id) {
            self.expanded_batches.insert(batch_id);
        }
        self.rebuild_display_rows();
    }

    fn rebuild_display_rows(&mut self) {
        let prev_act_id = self.selected_row().map(|r| r.activity_id().clone());
        let mut display = Vec::new();
        let mut i = 0;

        while i < self.raw_events.len() {
            let current = &self.raw_events[i];
            if current.batch_id != 0 {
                // Count consecutive events in the same batch by the same actor across >3 distinct issues
                let mut j = i + 1;
                let mut distinct_issues = HashSet::new();
                distinct_issues.insert(&current.issue_id);
                while j < self.raw_events.len()
                    && self.raw_events[j].batch_id == current.batch_id
                    && self.raw_events[j].actor_name == current.actor_name
                {
                    distinct_issues.insert(&self.raw_events[j].issue_id);
                    j += 1;
                }

                let group_len = j - i;
                if distinct_issues.len() > 3 {
                    let expanded = self.expanded_batches.contains(&current.batch_id);
                    display.push(FeedRowItem::BatchSummary {
                        batch_id: current.batch_id,
                        team_key: current.team_key.clone(),
                        actor_name: current.actor_name.clone(),
                        count: group_len,
                        expanded,
                        first_event: current.clone(),
                    });
                    if expanded {
                        for ev in &self.raw_events[i..j] {
                            display.push(FeedRowItem::Single(ev.clone()));
                        }
                    }
                    i = j;
                    continue;
                }
            }

            display.push(FeedRowItem::Single(current.clone()));
            i += 1;
        }

        self.display_rows = display;
        if self.display_rows.is_empty() {
            self.table_state.select(None);
            return;
        }

        if let Some(aid) = prev_act_id {
            if let Some(pos) = self
                .display_rows
                .iter()
                .position(|r| r.activity_id() == &aid)
            {
                self.table_state.select(Some(pos));
                return;
            }
        }

        let clamped = self
            .table_state
            .selected()
            .unwrap_or(0)
            .min(self.display_rows.len().saturating_sub(1));
        self.table_state.select(Some(clamped));
    }

    pub fn next(&mut self) {
        if self.display_rows.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) => (i + 1).min(self.display_rows.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn previous(&mut self) {
        if self.display_rows.is_empty() {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    pub fn page_down(&mut self) {
        if self.display_rows.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) => (i + 5).min(self.display_rows.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn page_up(&mut self) {
        if self.display_rows.is_empty() {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(5),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    pub fn first(&mut self) {
        if !self.display_rows.is_empty() {
            self.table_state.select(Some(0));
        }
    }

    pub fn last(&mut self) {
        if !self.display_rows.is_empty() {
            self.table_state.select(Some(self.display_rows.len() - 1));
        }
    }
}

pub struct ActivityFeedWidget {
    pub focused: bool,
}

impl ActivityFeedWidget {
    pub fn new(focused: bool) -> Self {
        Self { focused }
    }
}

impl StatefulWidget for ActivityFeedWidget {
    type State = ActivityFeedState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.focused {
            theme().table_border_focused
        } else {
            theme().table_border
        };

        let title = Line::from(vec![
            Span::styled(" Activity Feed ", theme().border_title),
            Span::styled(
                format!(
                    "({} events, sorted by When↓ — z/Space to expand [+] batches) ",
                    state.raw_events.len()
                ),
                theme().date,
            ),
        ]);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(theme().background);

        let header_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let header = Row::new(vec![
            Cell::from("When"),
            Cell::from("Team"),
            Cell::from("Issue"),
            Cell::from("Actor"),
            Cell::from("Activity / Change"),
        ])
        .style(header_style)
        .height(1);

        let rows = state.display_rows.iter().enumerate().map(|(idx, item)| {
            let row_bg = theme().table_row(idx);
            match item {
                FeedRowItem::Single(ev) => Row::new(vec![
                    Cell::from(Span::styled(
                        format_relative_time(ev.timestamp).to_string(),
                        theme().date,
                    )),
                    Cell::from(Span::styled(ev.team_key.as_str(), theme().team_badge)),
                    Cell::from(Span::styled(ev.issue_identifier.as_str(), theme().issue_id)),
                    Cell::from(Span::styled(
                        ev.actor_name.as_str(),
                        theme().activity_author,
                    )),
                    Cell::from(Line::from(vec![
                        Span::styled(format!("{} — ", ev.summary), theme().activity_action),
                        Span::styled(ev.issue_title.as_str(), theme().issue_title),
                    ])),
                ])
                .style(row_bg),
                FeedRowItem::BatchSummary {
                    team_key,
                    actor_name,
                    count,
                    expanded,
                    first_event,
                    ..
                } => {
                    let marker = if *expanded { "[-]" } else { "[+]" };
                    Row::new(vec![
                        Cell::from(Span::styled(
                            format_relative_time(first_event.timestamp).to_string(),
                            theme().date,
                        )),
                        Cell::from(Span::styled(team_key.as_str(), theme().team_badge)),
                        Cell::from(Span::styled(marker, theme().health_warn)),
                        Cell::from(Span::styled(
                            actor_name.as_str(),
                            theme().activity_author,
                        )),
                        Cell::from(Span::styled(
                            format!(
                                "{marker} {count} issues updated in batch (press z or Space to toggle)"
                            ),
                            theme().project_name,
                        )),
                    ])
                    .style(row_bg)
                }
            }
        });

        let widths = [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Min(36),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(theme().highlight_symbol)
            .highlight_symbol("▶ ");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}
