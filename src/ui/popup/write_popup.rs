use compact_str::CompactString;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, StatefulWidget, Table, TableState, Widget, Wrap},
};

use crate::{
    domain::{
        Issue, IssueLabel, IssuePriority, LinearMutation, Project, StateCategory, Team, User,
        WorkflowState,
    },
    gruvbox::Gruvbox,
    id::{IssueId, LabelId, ProjectId, StateId, TeamId, UserId},
    stores::LinearStore,
    theme::theme,
    ui::{fx::window::Window, popup::utility::CenteredShrink, widget::input_field::TextEditor},
};

#[derive(Debug, Clone)]
pub enum QuickPickerAction {
    SetState {
        state_id: StateId,
        state_name: CompactString,
        state_category: StateCategory,
    },
    SetPriority(IssuePriority),
    SetAssignee {
        user_id: Option<UserId>,
        user_name: CompactString,
    },
    SetProject {
        project_id: Option<ProjectId>,
        project_name: CompactString,
    },
}

#[derive(Debug, Clone)]
pub struct QuickPickerOption {
    pub badge: CompactString,
    pub label: CompactString,
    pub label_style: Style,
    pub is_current: bool,
    pub hotkey: Option<char>,
    pub action: QuickPickerAction,
}

#[derive(Debug, Clone)]
pub struct QuickPickerPopup {
    pub title: CompactString,
    pub col_label: CompactString,
    pub col_meta: CompactString,
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub issue_title: CompactString,
    pub targets: Vec<Issue>,
    pub all_states: Vec<WorkflowState>,
    pub options: Vec<QuickPickerOption>,
    pub filter_query: String,
    pub selected_idx: usize,
}

fn summarize_targets(targets: &[Issue]) -> (IssueId, CompactString, CompactString) {
    if let [single] = targets {
        (
            single.id.clone(),
            single.identifier.clone(),
            single.title.clone(),
        )
    } else {
        let first_id = targets
            .first()
            .map(|i| i.id.clone())
            .unwrap_or_default();
        let ident: CompactString = format!("{} issues selected", targets.len()).into();
        let keys: Vec<&str> = targets.iter().map(|i| i.identifier.as_str()).collect();
        let summary_title: CompactString = keys.join(", ").into();
        (first_id, ident, summary_title)
    }
}

fn human_state_category_badge(st: &WorkflowState) -> &'static str {
    if st.name.eq_ignore_ascii_case("Duplicate") {
        return "Canceled";
    }
    match st.category {
        StateCategory::Triage => "Triage",
        StateCategory::Backlog => "Backlog",
        StateCategory::Unstarted => "Todo",
        StateCategory::Started => "In Progress",
        StateCategory::Completed => "Done",
        StateCategory::Canceled => "Canceled",
    }
}

impl QuickPickerPopup {
    pub fn new_state_picker(issue: &Issue, store: &LinearStore) -> Self {
        Self::new_state_picker_for_targets(vec![issue.clone()], store)
    }

    pub fn new_state_picker_for_targets(targets: Vec<Issue>, store: &LinearStore) -> Self {
        let primary = targets.first().expect("targets must not be empty");
        let states = store.states_for_team(&primary.team_id);
        let all_states: Vec<WorkflowState> = store.states.values().cloned().collect();
        let mut selected_idx = 0;
        let options: Vec<QuickPickerOption> = states
            .into_iter()
            .enumerate()
            .map(|(idx, st)| {
                let is_current = targets.len() == 1 && st.id == primary.state_id;
                if is_current {
                    selected_idx = idx;
                }
                let hotkey = if idx < 9 {
                    char::from_digit((idx + 1) as u32, 10)
                } else {
                    None
                };
                let label_style = theme().state_category_style(st.category, Some(&st.color));
                QuickPickerOption {
                    badge: human_state_category_badge(&st).into(),
                    label: st.name.clone(),
                    label_style,
                    is_current,
                    hotkey,
                    action: QuickPickerAction::SetState {
                        state_id: st.id,
                        state_name: st.name,
                        state_category: st.category,
                    },
                }
            })
            .collect();

        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);
        Self {
            title: "Change Workflow State".into(),
            col_label: "Workflow State".into(),
            col_meta: "Category".into(),
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            all_states,
            options,
            filter_query: String::new(),
            selected_idx,
        }
    }

    pub fn new_priority_picker(issue: &Issue) -> Self {
        Self::new_priority_picker_for_targets(vec![issue.clone()])
    }

    pub fn new_priority_picker_for_targets(targets: Vec<Issue>) -> Self {
        let primary = targets.first().expect("targets must not be empty");
        let priorities = [
            (IssuePriority::Urgent, '1'),
            (IssuePriority::High, '2'),
            (IssuePriority::Medium, '3'),
            (IssuePriority::Low, '4'),
            (IssuePriority::NoPriority, '0'),
        ];
        let mut selected_idx = 0;
        let options: Vec<QuickPickerOption> = priorities
            .into_iter()
            .enumerate()
            .map(|(idx, (pri, key))| {
                let is_current = targets.len() == 1 && primary.priority == pri;
                if is_current {
                    selected_idx = idx;
                }
                QuickPickerOption {
                    badge: pri.short_label().into(),
                    label: pri.label().into(),
                    label_style: theme().priority_style(pri),
                    is_current,
                    hotkey: Some(key),
                    action: QuickPickerAction::SetPriority(pri),
                }
            })
            .collect();

        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);
        Self {
            title: "Change Priority".into(),
            col_label: "Priority Level".into(),
            col_meta: "Code".into(),
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            all_states: Vec::new(),
            options,
            filter_query: String::new(),
            selected_idx,
        }
    }

    pub fn new_assignee_picker(issue: &Issue, store: &LinearStore) -> Self {
        Self::new_assignee_picker_for_targets(vec![issue.clone()], store)
    }

    pub fn new_assignee_picker_for_targets(targets: Vec<Issue>, store: &LinearStore) -> Self {
        let primary = targets.first().expect("targets must not be empty");
        let viewer_id = store.viewer.as_ref().map(|v| &v.id);
        let users = store.sorted_users();
        let mut options = Vec::with_capacity(users.len() + 1);
        let mut selected_idx = 0;

        options.push(QuickPickerOption {
            badge: "None".into(),
            label: "Unassigned".into(),
            label_style: theme().input_description,
            is_current: targets.len() == 1 && primary.assignee_id.is_none(),
            hotkey: Some('0'),
            action: QuickPickerAction::SetAssignee {
                user_id: None,
                user_name: "Unassigned".into(),
            },
        });

        for (idx, user) in users.into_iter().enumerate() {
            let is_me = viewer_id == Some(&user.id);
            let is_current = targets.len() == 1 && primary.assignee_id.as_ref() == Some(&user.id);
            if is_current {
                selected_idx = options.len();
            }
            let badge: CompactString = if is_me {
                "You".into()
            } else {
                "Member".into()
            };
            let label: CompactString = if user.display_name != user.name {
                format!("{} (@{})", user.display_name, user.name).into()
            } else {
                user.display_name.clone()
            };
            let hotkey = if idx < 9 {
                char::from_digit((idx + 1) as u32, 10)
            } else {
                None
            };
            options.push(QuickPickerOption {
                badge,
                label,
                label_style: theme().assignee,
                is_current,
                hotkey,
                action: QuickPickerAction::SetAssignee {
                    user_id: Some(user.id),
                    user_name: user.display_name,
                },
            });
        }

        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);
        Self {
            title: "Assign Issue".into(),
            col_label: "Assignee".into(),
            col_meta: "Role".into(),
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            all_states: Vec::new(),
            options,
            filter_query: String::new(),
            selected_idx,
        }
    }

    pub fn new_project_picker(issue: &Issue, store: &LinearStore) -> Self {
        Self::new_project_picker_for_targets(vec![issue.clone()], store)
    }

    pub fn new_project_picker_for_targets(targets: Vec<Issue>, store: &LinearStore) -> Self {
        let primary = targets.first().expect("targets must not be empty");
        let projects = store.projects_for_team(&primary.team_id);
        let mut options = Vec::with_capacity(projects.len() + 1);
        let mut selected_idx = 0;

        options.push(QuickPickerOption {
            badge: "—".into(),
            label: "No Project (Unassigned)".into(),
            label_style: theme().input_description,
            is_current: targets.len() == 1 && primary.project_id.is_none(),
            hotkey: Some('0'),
            action: QuickPickerAction::SetProject {
                project_id: None,
                project_name: "Unassigned".into(),
            },
        });

        for (idx, proj) in projects.into_iter().enumerate() {
            let is_current = targets.len() == 1 && primary.project_id.as_ref() == Some(&proj.id);
            if is_current {
                selected_idx = options.len();
            }
            let pct = (proj.progress * 100.0).round() as u32;
            let hotkey = if idx < 9 {
                char::from_digit((idx + 1) as u32, 10)
            } else {
                None
            };
            options.push(QuickPickerOption {
                badge: format!("{pct:>3}%").into(),
                label: proj.name.clone(),
                label_style: theme().project_name,
                is_current,
                hotkey,
                action: QuickPickerAction::SetProject {
                    project_id: Some(proj.id),
                    project_name: proj.name,
                },
            });
        }

        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);
        Self {
            title: "Move Issue to Project".into(),
            col_label: "Project".into(),
            col_meta: "Progress".into(),
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            all_states: Vec::new(),
            options,
            filter_query: String::new(),
            selected_idx,
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.filter_query.trim().to_ascii_lowercase();
        self.options
            .iter()
            .enumerate()
            .filter(|(_, opt)| {
                q.is_empty()
                    || opt.label.to_ascii_lowercase().contains(&q)
                    || opt.badge.to_ascii_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn move_down(&mut self, step: usize) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.selected_idx = 0;
            return;
        }
        self.selected_idx = (self.selected_idx + step).min(indices.len() - 1);
    }

    pub fn move_up(&mut self, step: usize) {
        self.selected_idx = self.selected_idx.saturating_sub(step);
    }

    pub fn push_filter_char(&mut self, ch: char) {
        self.filter_query.push(ch);
        self.selected_idx = 0;
    }

    pub fn pop_filter_char(&mut self) {
        self.filter_query.pop();
        self.selected_idx = 0;
    }

    pub fn clear_filter(&mut self) {
        self.filter_query.clear();
        self.selected_idx = 0;
    }

    pub fn build_mutation_for_option(&self, opt: &QuickPickerOption) -> LinearMutation {
        if self.targets.len() <= 1 {
            return match &opt.action {
                QuickPickerAction::SetState {
                    state_id,
                    state_name,
                    ..
                } => LinearMutation::UpdateIssue {
                    issue_id: self.issue_id.clone(),
                    title: None,
                    description: None,
                    state_id: Some(state_id.clone()),
                    priority: None,
                    assignee_id: None,
                    project_id: None,
                    label_ids: None,
                    summary_label: format!("{}: State -> {state_name}", self.issue_identifier)
                        .into(),
                },
                QuickPickerAction::SetPriority(pri) => LinearMutation::UpdateIssue {
                    issue_id: self.issue_id.clone(),
                    title: None,
                    description: None,
                    state_id: None,
                    priority: Some(*pri),
                    assignee_id: None,
                    project_id: None,
                    label_ids: None,
                    summary_label: format!(
                        "{}: Priority -> {}",
                        self.issue_identifier,
                        pri.label()
                    )
                    .into(),
                },
                QuickPickerAction::SetAssignee { user_id, user_name } => {
                    LinearMutation::UpdateIssue {
                        issue_id: self.issue_id.clone(),
                        title: None,
                        description: None,
                        state_id: None,
                        priority: None,
                        assignee_id: Some(user_id.clone()),
                        project_id: None,
                        label_ids: None,
                        summary_label: format!("{}: Assignee -> {user_name}", self.issue_identifier)
                            .into(),
                    }
                }
                QuickPickerAction::SetProject {
                    project_id,
                    project_name,
                } => LinearMutation::UpdateIssue {
                    issue_id: self.issue_id.clone(),
                    title: None,
                    description: None,
                    state_id: None,
                    priority: None,
                    assignee_id: None,
                    project_id: Some(project_id.clone()),
                    label_ids: None,
                    summary_label: format!("{}: Project -> {project_name}", self.issue_identifier)
                        .into(),
                },
            };
        }

        let count = self.targets.len();
        match &opt.action {
            QuickPickerAction::SetState {
                state_id,
                state_name,
                state_category,
            } => {
                let mutations = self
                    .targets
                    .iter()
                    .map(|issue| {
                        let is_todo_alias = state_name.eq_ignore_ascii_case("Todo")
                            || state_name.eq_ignore_ascii_case("Planned");
                        let resolved_state_id = self
                            .all_states
                            .iter()
                            .find(|s| s.team_id == issue.team_id && s.id == *state_id)
                            .or_else(|| {
                                self.all_states.iter().find(|s| {
                                    s.team_id == issue.team_id
                                        && s.name.eq_ignore_ascii_case(state_name.as_str())
                                })
                            })
                            .or_else(|| {
                                if is_todo_alias {
                                    self.all_states.iter().find(|s| {
                                        s.team_id == issue.team_id
                                            && (s.name.eq_ignore_ascii_case("Todo")
                                                || s.name.eq_ignore_ascii_case("Planned"))
                                    })
                                } else {
                                    None
                                }
                            })
                            .or_else(|| {
                                self.all_states.iter().find(|s| {
                                    s.team_id == issue.team_id
                                        && s.category == *state_category
                                        && !s.name.eq_ignore_ascii_case("Duplicate")
                                        && !s.name.eq_ignore_ascii_case("Blocked")
                                })
                            })
                            .or_else(|| {
                                self.all_states.iter().find(|s| {
                                    s.team_id == issue.team_id && s.category == *state_category
                                })
                            })
                            .map(|s| s.id.clone())
                            .unwrap_or_else(|| state_id.clone());

                        LinearMutation::UpdateIssue {
                            issue_id: issue.id.clone(),
                            title: None,
                            description: None,
                            state_id: Some(resolved_state_id),
                            priority: None,
                            assignee_id: None,
                            project_id: None,
                            label_ids: None,
                            summary_label: format!("{}: State -> {state_name}", issue.identifier)
                                .into(),
                        }
                    })
                    .collect();
                LinearMutation::Batch {
                    mutations,
                    summary_label: format!("Updated state -> {state_name} on {count} issues")
                        .into(),
                }
            }
            QuickPickerAction::SetPriority(pri) => {
                let mutations = self
                    .targets
                    .iter()
                    .map(|issue| LinearMutation::UpdateIssue {
                        issue_id: issue.id.clone(),
                        title: None,
                        description: None,
                        state_id: None,
                        priority: Some(*pri),
                        assignee_id: None,
                        project_id: None,
                        label_ids: None,
                        summary_label: format!("{}: Priority -> {}", issue.identifier, pri.label())
                            .into(),
                    })
                    .collect();
                LinearMutation::Batch {
                    mutations,
                    summary_label: format!("Updated priority -> {} on {count} issues", pri.label())
                        .into(),
                }
            }
            QuickPickerAction::SetAssignee { user_id, user_name } => {
                let mutations = self
                    .targets
                    .iter()
                    .map(|issue| LinearMutation::UpdateIssue {
                        issue_id: issue.id.clone(),
                        title: None,
                        description: None,
                        state_id: None,
                        priority: None,
                        assignee_id: Some(user_id.clone()),
                        project_id: None,
                        label_ids: None,
                        summary_label: format!("{}: Assignee -> {user_name}", issue.identifier)
                            .into(),
                    })
                    .collect();
                LinearMutation::Batch {
                    mutations,
                    summary_label: format!("Assigned {count} issues -> {user_name}").into(),
                }
            }
            QuickPickerAction::SetProject {
                project_id,
                project_name,
            } => {
                let mutations = self
                    .targets
                    .iter()
                    .map(|issue| LinearMutation::UpdateIssue {
                        issue_id: issue.id.clone(),
                        title: None,
                        description: None,
                        state_id: None,
                        priority: None,
                        assignee_id: None,
                        project_id: Some(project_id.clone()),
                        label_ids: None,
                        summary_label: format!("{}: Project -> {project_name}", issue.identifier)
                            .into(),
                    })
                    .collect();
                LinearMutation::Batch {
                    mutations,
                    summary_label: format!("Moved {count} issues -> {project_name}").into(),
                }
            }
        }
    }

    pub fn selected_mutation(&self) -> Option<LinearMutation> {
        let indices = self.filtered_indices();
        let real_idx = *indices.get(self.selected_idx)?;
        let opt = self.options.get(real_idx)?;
        Some(self.build_mutation_for_option(opt))
    }

    pub fn hotkey_mutation(&self, ch: char) -> Option<LinearMutation> {
        if !self.filter_query.is_empty() {
            return None;
        }
        let opt = self.options.iter().find(|o| o.hotkey == Some(ch))?;
        Some(self.build_mutation_for_option(opt))
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        let w = 68u16.min(terminal_area.width.saturating_sub(4));
        let h = 18u16.min(terminal_area.height.saturating_sub(4));
        let x = terminal_area.x + terminal_area.width.saturating_sub(w) / 2;
        let y = terminal_area.y + terminal_area.height.saturating_sub(h) / 2;
        Rect::new(x, y, w, h)
    }

    fn sized_popup_area(&self, terminal_area: Rect) -> Rect {
        let w = 68u16.min(terminal_area.width.saturating_sub(4));
        let desired_h = (self.options.len() as u16 + 8).clamp(11, 22);
        let h = desired_h.min(terminal_area.height.saturating_sub(4));
        let x = terminal_area.x + terminal_area.width.saturating_sub(w) / 2;
        let y = terminal_area.y + terminal_area.height.saturating_sub(h) / 2;
        Rect::new(x, y, w, h)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = self.sized_popup_area(area);
        let title = Line::from(vec![
            Span::styled(format!(" {} ", self.title), theme().border.title),
            Span::styled(
                format!(" {} ", self.issue_identifier),
                theme().issue_id,
            ),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        if inner.height < 4 || inner.width < 16 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        let target_prefix = if self.targets.len() > 1 {
            format!("Targets ({}): ", self.targets.len())
        } else {
            "Issue: ".to_string()
        };
        let filter_display = if self.filter_query.is_empty() {
            "1-9/0: quick pick • ↑/↓: move • type: filter • Enter: apply • Esc: cancel".to_string()
        } else {
            format!("Filter: {}█  (Backspace to edit, Esc to clear)", self.filter_query)
        };
        let sep = "─".repeat(chunks[0].width as usize);
        let top_lines = vec![
            Line::from(vec![
                Span::styled(target_prefix, theme().input_label),
                Span::styled(self.issue_title.as_str(), theme().issue_title),
            ]),
            Line::from(Span::styled(filter_display, theme().input_description)),
            Line::from(Span::styled(sep, Style::default().fg(Gruvbox::Dark3.into()))),
        ];
        Paragraph::new(top_lines).render(chunks[0], buf);

        let indices = self.filtered_indices();
        if indices.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "  (no matching options)",
                theme().input_description,
            )))
            .render(chunks[1], buf);
            return;
        }

        let header_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let header = Row::new(vec![
            Cell::from("Key"),
            Cell::from(self.col_label.as_str()),
            Cell::from(self.col_meta.as_str()),
            Cell::from("Status"),
        ])
        .style(header_style)
        .height(1);

        let current_style = Style::default()
            .fg(Gruvbox::GreenBright.into())
            .add_modifier(Modifier::BOLD);

        let rows = indices.iter().enumerate().map(|(vis_idx, &real_idx)| {
            let opt = &self.options[real_idx];
            let is_sel = vis_idx == self.selected_idx;
            let row_bg = theme().table_row(vis_idx);

            let key_str = match opt.hotkey {
                Some(k) => format!("[{k}]"),
                None => " — ".to_string(),
            };
            let key_style = if is_sel {
                Style::default()
                    .fg(Gruvbox::YellowBright.into())
                    .add_modifier(Modifier::BOLD)
            } else {
                theme().issue_id
            };
            let lbl_style = if is_sel {
                opt.label_style.add_modifier(Modifier::BOLD)
            } else {
                opt.label_style
            };
            let status_cell = if opt.is_current {
                Cell::from(Span::styled("● current", current_style))
            } else {
                Cell::from(Span::styled("", theme().input_description))
            };

            Row::new(vec![
                Cell::from(Span::styled(key_str, key_style)),
                Cell::from(Span::styled(opt.label.as_str(), lbl_style)),
                Cell::from(Span::styled(opt.badge.as_str(), theme().date)),
                status_cell,
            ])
            .style(row_bg)
        });

        let widths = [
            Constraint::Length(6),
            Constraint::Min(24),
            Constraint::Length(14),
            Constraint::Length(11),
        ];

        let mut table_state = TableState::default().with_selected(Some(self.selected_idx));
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(theme().highlight_symbol)
            .highlight_symbol("▶ ");

        StatefulWidget::render(table, chunks[1], buf, &mut table_state);
    }
}

#[derive(Debug, Clone)]
pub struct LabelPickerPopup {
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub issue_title: CompactString,
    pub targets: Vec<Issue>,
    pub team_id: TeamId,
    pub available_labels: Vec<IssueLabel>,
    pub selected_label_ids: Vec<LabelId>,
    pub cursor_idx: usize,
    pub creating_new: bool,
    pub new_label_name: String,
}

impl LabelPickerPopup {
    pub fn new(issue: &Issue, store: &LinearStore) -> Self {
        Self::new_for_targets(vec![issue.clone()], store)
    }

    pub fn new_for_targets(targets: Vec<Issue>, store: &LinearStore) -> Self {
        let primary = targets.first().expect("targets must not be empty");
        let available_labels = store.labels_for_team(&primary.team_id);
        let mut selected_label_ids: Vec<LabelId> = primary
            .label_ids
            .iter()
            .filter(|id| !id.as_str().starts_with("name:"))
            .cloned()
            .collect();

        // Also match by name if label_ids wasn't populated yet
        for avail in &available_labels {
            if primary
                .labels
                .iter()
                .any(|n| n.eq_ignore_ascii_case(avail.name.as_str()))
                && !selected_label_ids.contains(&avail.id)
            {
                selected_label_ids.push(avail.id.clone());
            }
        }

        let team_id = primary.team_id.clone();
        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);

        Self {
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            team_id,
            available_labels,
            selected_label_ids,
            cursor_idx: 0,
            creating_new: false,
            new_label_name: String::new(),
        }
    }

    pub fn move_down(&mut self, step: usize) {
        if !self.available_labels.is_empty() {
            self.cursor_idx = (self.cursor_idx + step).min(self.available_labels.len() - 1);
        }
    }

    pub fn move_up(&mut self, step: usize) {
        self.cursor_idx = self.cursor_idx.saturating_sub(step);
    }

    pub fn toggle_current(&mut self) {
        let Some(lbl) = self.available_labels.get(self.cursor_idx) else {
            return;
        };
        if let Some(pos) = self.selected_label_ids.iter().position(|id| id == &lbl.id) {
            self.selected_label_ids.remove(pos);
        } else {
            self.selected_label_ids.push(lbl.id.clone());
        }
    }

    pub fn build_mutation(&self) -> Option<LinearMutation> {
        if self.creating_new {
            let trimmed = self.new_label_name.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(LinearMutation::CreateLabelAndAttach {
                issue_id: self.issue_id.clone(),
                team_id: self.team_id.clone(),
                label_name: CompactString::from(trimmed),
                existing_label_ids: self.selected_label_ids.clone(),
            });
        }

        let names: Vec<&str> = self
            .available_labels
            .iter()
            .filter(|l| self.selected_label_ids.contains(&l.id))
            .map(|l| l.name.as_str())
            .collect();

        if self.targets.len() <= 1 {
            let summary = if names.is_empty() {
                format!("{}: Cleared labels", self.issue_identifier)
            } else {
                format!("{}: Labels -> {}", self.issue_identifier, names.join(", "))
            };

            return Some(LinearMutation::UpdateIssue {
                issue_id: self.issue_id.clone(),
                title: None,
                description: None,
                state_id: None,
                priority: None,
                assignee_id: None,
                project_id: None,
                label_ids: Some(self.selected_label_ids.clone()),
                summary_label: summary.into(),
            });
        }

        let count = self.targets.len();
        let mutations = self
            .targets
            .iter()
            .map(|issue| {
                let summary = if names.is_empty() {
                    format!("{}: Cleared labels", issue.identifier)
                } else {
                    format!("{}: Labels -> {}", issue.identifier, names.join(", "))
                };
                LinearMutation::UpdateIssue {
                    issue_id: issue.id.clone(),
                    title: None,
                    description: None,
                    state_id: None,
                    priority: None,
                    assignee_id: None,
                    project_id: None,
                    label_ids: Some(self.selected_label_ids.clone()),
                    summary_label: summary.into(),
                }
            })
            .collect();
        let batch_summary = if names.is_empty() {
            format!("Cleared labels on {count} issues")
        } else {
            format!("Updated labels -> {} on {count} issues", names.join(", "))
        };
        Some(LinearMutation::Batch {
            mutations,
            summary_label: batch_summary.into(),
        })
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        let w = 66u16.min(terminal_area.width.saturating_sub(4));
        let h = 20u16.min(terminal_area.height.saturating_sub(4));
        let x = terminal_area.x + terminal_area.width.saturating_sub(w) / 2;
        let y = terminal_area.y + terminal_area.height.saturating_sub(h) / 2;
        Rect::new(x, y, w, h)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(" Manage Issue Labels ", theme().border.title),
            Span::styled(format!(" {} ", self.issue_identifier), theme().issue_id),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        if inner.height < 6 || inner.width < 16 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(inner);

        let target_prefix = if self.targets.len() > 1 {
            format!("Targets ({}): ", self.targets.len())
        } else {
            "Issue: ".to_string()
        };
        let sep = "─".repeat(chunks[0].width as usize);
        let top_lines = vec![
            Line::from(vec![
                Span::styled(target_prefix, theme().input_label),
                Span::styled(self.issue_title.as_str(), theme().issue_title),
            ]),
            Line::from(Span::styled(
                "Space: toggle label • +: create new label • Enter: apply • Esc: cancel",
                theme().input_description,
            )),
            Line::from(Span::styled(sep.clone(), Style::default().fg(Gruvbox::Dark3.into()))),
        ];
        Paragraph::new(top_lines).render(chunks[0], buf);

        if self.available_labels.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "  (No cached team labels yet — press '+' to create and attach a new label)",
                theme().input_description,
            )))
            .render(chunks[1], buf);
        } else {
            let header_style = Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD);
            let header = Row::new(vec![
                Cell::from("Applied"),
                Cell::from("Label Name"),
                Cell::from("Scope"),
            ])
            .style(header_style)
            .height(1);

            let check_style = Style::default()
                .fg(Gruvbox::GreenBright.into())
                .add_modifier(Modifier::BOLD);

            let rows = self.available_labels.iter().enumerate().map(|(idx, lbl)| {
                let checked = self.selected_label_ids.contains(&lbl.id);
                let row_bg = theme().table_row(idx);
                let box_cell = if checked {
                    Cell::from(Span::styled("  [✓]", check_style))
                } else {
                    Cell::from(Span::styled("  [ ]", theme().date))
                };
                let name_style = if checked {
                    theme().label_badge.add_modifier(Modifier::BOLD)
                } else {
                    theme().issue_title
                };
                let scope_str = if lbl.team_id.is_some() {
                    "Team"
                } else {
                    "Workspace"
                };

                Row::new(vec![
                    box_cell,
                    Cell::from(Span::styled(lbl.name.as_str(), name_style)),
                    Cell::from(Span::styled(scope_str, theme().date)),
                ])
                .style(row_bg)
            });

            let widths = [
                Constraint::Length(9),
                Constraint::Min(28),
                Constraint::Length(12),
            ];

            let selected_row = if self.creating_new {
                None
            } else {
                Some(self.cursor_idx)
            };
            let mut table_state = TableState::default().with_selected(selected_row);
            let table = Table::new(rows, widths)
                .header(header)
                .row_highlight_style(theme().highlight_symbol)
                .highlight_symbol("▶ ");

            StatefulWidget::render(table, chunks[1], buf, &mut table_state);
        }

        let footer_line = if self.creating_new {
            Line::from(vec![
                Span::styled("▶ New Label Name: ", theme().issue_id),
                Span::styled(
                    format!(" {}█ ", self.new_label_name),
                    theme().input_selected,
                ),
            ])
        } else {
            Line::from(Span::styled(
                "Press '+' to type and create a new label on this team",
                theme().input_description,
            ))
        };
        Paragraph::new(vec![
            Line::from(Span::styled(sep, Style::default().fg(Gruvbox::Dark3.into()))),
            footer_line,
        ])
        .render(chunks[2], buf);
    }
}

#[derive(Debug, Clone)]
pub struct CommentComposePopup {
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub issue_title: CompactString,
    pub targets: Vec<Issue>,
    pub editor: TextEditor,
    pub body: String,
    pub scroll: u16,
    pub is_draft: bool,
    pub esc_pending: bool,
}

impl CommentComposePopup {
    pub fn new(issue: &Issue) -> Self {
        Self::new_for_targets(vec![issue.clone()])
    }

    pub fn new_for_targets(targets: Vec<Issue>) -> Self {
        Self::new_for_targets_with_draft(targets, None)
    }

    pub fn new_for_targets_with_draft(targets: Vec<Issue>, initial_draft: Option<&str>) -> Self {
        let (issue_id, issue_identifier, issue_title) = summarize_targets(&targets);
        let draft_str = initial_draft.unwrap_or("").to_string();
        let is_draft = !draft_str.trim().is_empty();
        let editor = TextEditor::from_text(draft_str.clone());
        Self {
            issue_id,
            issue_identifier,
            issue_title,
            targets,
            editor,
            body: draft_str,
            scroll: 0,
            is_draft,
            esc_pending: false,
        }
    }

    pub fn sync_body_from_editor(&mut self) {
        self.body = self.editor.text.clone();
    }

    pub fn set_body(&mut self, text: impl Into<String>) {
        let s = text.into();
        self.body = s.clone();
        self.editor.set_text(s);
    }

    pub fn clear_draft_text(&mut self) {
        self.editor.clear();
        self.body.clear();
        self.is_draft = false;
        self.esc_pending = false;
    }

    pub fn build_mutation(&mut self) -> Option<LinearMutation> {
        if self.editor.text != self.body && !self.body.is_empty() && self.editor.is_empty() {
            self.editor.set_text(self.body.clone());
        } else {
            self.sync_body_from_editor();
        }
        let trimmed = self.body.trim();
        if trimmed.is_empty() {
            return None;
        }
        if self.targets.len() <= 1 {
            return Some(LinearMutation::CreateComment {
                issue_id: self.issue_id.clone(),
                body: trimmed.to_string(),
            });
        }
        let count = self.targets.len();
        let mutations = self
            .targets
            .iter()
            .map(|issue| LinearMutation::CreateComment {
                issue_id: issue.id.clone(),
                body: trimmed.to_string(),
            })
            .collect();
        Some(LinearMutation::Batch {
            mutations,
            summary_label: format!("Posted comment on {count} issues").into(),
        })
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(72, 68)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let mut title_spans = vec![
            Span::styled(" Post Comment ", theme().border.title),
            Span::styled(format!(" {} ", self.issue_identifier), theme().issue_id),
        ];
        if self.is_draft && !self.editor.text.trim().is_empty() {
            title_spans.push(Span::styled(" [DRAFT] ", theme().health_warn));
        }
        if self.esc_pending {
            title_spans.push(Span::styled(
                " [ESC: Enter=post • Esc=close & keep draft] ",
                theme().active_tab,
            ));
        }
        let title = Line::from(title_spans);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let mut lines = Vec::new();
        let target_prefix = if self.targets.len() > 1 {
            format!("Targets ({}): ", self.targets.len())
        } else {
            "Issue: ".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(target_prefix, theme().input_label),
            Span::styled(self.issue_title.as_str(), theme().issue_title),
        ]));
        let help_text = if self.esc_pending {
            "ESC chord active: press Enter to post comment • press Esc to close (non-blank text is saved as a draft)"
        } else {
            "Ins: $EDITOR • Esc+Enter: post • Esc+Esc: close & save draft • Ctrl+x: delete draft • Emacs keys (Ctrl+a/e/j/k/u/w)"
        };
        lines.push(Line::from(Span::styled(
            help_text,
            if self.esc_pending {
                theme().health_warn
            } else {
                theme().input_description
            },
        )));
        lines.push(Line::from(Span::styled(
            "────────────────────────────────────────────────────────────────────────────",
            theme().input_description,
        )));

        lines.extend(self.editor.render_lines(theme().issue_title));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(inner, buf);
    }
}

#[derive(Debug, Clone)]
pub struct IssueFormPopup {
    pub edit_issue_id: Option<IssueId>,
    pub edit_identifier: Option<CompactString>,
    pub teams: Vec<Team>,
    pub team_idx: usize,
    pub title_editor: TextEditor,
    pub title: String,
    pub all_states: Vec<WorkflowState>,
    pub states: Vec<WorkflowState>,
    pub state_idx: usize,
    pub priority: IssuePriority,
    pub users: Vec<User>,
    pub assignee_idx: usize, // 0 = Unassigned, 1..=users.len()
    pub all_projects: Vec<Project>,
    pub projects: Vec<Project>,
    pub project_idx: usize, // 0 = Unassigned, 1..=projects.len()
    pub description_editor: TextEditor,
    pub description: String,
    pub selected_field: usize,
    pub esc_pending: bool,
}

const ISSUE_FORM_FIELDS: usize = 8;

impl IssueFormPopup {
    pub fn new_create(store: &LinearStore, active_team_key: Option<&str>) -> Self {
        let mut teams: Vec<Team> = store.teams.values().cloned().collect();
        teams.sort_by(|a, b| a.key.cmp(&b.key));

        let team_idx = active_team_key
            .and_then(|tk| teams.iter().position(|t| t.key.eq_ignore_ascii_case(tk)))
            .unwrap_or(0);

        let selected_team_id = teams
            .get(team_idx)
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let all_states: Vec<WorkflowState> = store.states.values().cloned().collect();
        let states = store.states_for_team(&selected_team_id);
        let state_idx = states
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case("Todo"))
            .unwrap_or(0);
        let all_projects: Vec<Project> = store.projects.values().cloned().collect();
        let projects = store.projects_for_team(&selected_team_id);
        let users = store.sorted_users();

        Self {
            edit_issue_id: None,
            edit_identifier: None,
            teams,
            team_idx,
            title_editor: TextEditor::new(),
            title: String::new(),
            all_states,
            states,
            state_idx,
            priority: IssuePriority::NoPriority,
            users,
            assignee_idx: 0,
            all_projects,
            projects,
            project_idx: 0,
            description_editor: TextEditor::new(),
            description: String::new(),
            selected_field: 1, // Focus Title immediately
            esc_pending: false,
        }
    }

    pub fn new_edit(issue: &Issue, store: &LinearStore) -> Self {
        let mut teams: Vec<Team> = store.teams.values().cloned().collect();
        teams.sort_by(|a, b| a.key.cmp(&b.key));
        let team_idx = teams
            .iter()
            .position(|t| t.id == issue.team_id)
            .unwrap_or(0);

        let all_states: Vec<WorkflowState> = store.states.values().cloned().collect();
        let states = store.states_for_team(&issue.team_id);
        let state_idx = states
            .iter()
            .position(|s| s.id == issue.state_id)
            .unwrap_or(0);

        let users = store.sorted_users();
        let assignee_idx = issue
            .assignee_id
            .as_ref()
            .and_then(|uid| users.iter().position(|u| &u.id == uid).map(|p| p + 1))
            .unwrap_or(0);

        let all_projects: Vec<Project> = store.projects.values().cloned().collect();
        let projects = store.projects_for_team(&issue.team_id);
        let project_idx = issue
            .project_id
            .as_ref()
            .and_then(|pid| projects.iter().position(|p| &p.id == pid).map(|p| p + 1))
            .unwrap_or(0);

        let title_str = issue.title.to_string();
        let desc_str = issue.description.clone().unwrap_or_default();

        Self {
            edit_issue_id: Some(issue.id.clone()),
            edit_identifier: Some(issue.identifier.clone()),
            teams,
            team_idx,
            title_editor: TextEditor::from_text(title_str.clone()),
            title: title_str,
            all_states,
            states,
            state_idx,
            priority: issue.priority,
            users,
            assignee_idx,
            all_projects,
            projects,
            project_idx,
            description_editor: TextEditor::from_text(desc_str.clone()),
            description: desc_str,
            selected_field: 1,
            esc_pending: false,
        }
    }

    pub fn sync_from_editors(&mut self) {
        self.title = self.title_editor.text.clone();
        self.description = self.description_editor.text.clone();
    }

    pub fn set_description(&mut self, text: impl Into<String>) {
        let s = text.into();
        self.description = s.clone();
        self.description_editor.set_text(s);
    }

    pub fn next_field(&mut self) {
        self.selected_field = (self.selected_field + 1) % ISSUE_FORM_FIELDS;
        if self.edit_issue_id.is_some() && self.selected_field == 0 {
            self.selected_field = 1;
        }
    }

    pub fn prev_field(&mut self) {
        self.selected_field = (self.selected_field + ISSUE_FORM_FIELDS - 1) % ISSUE_FORM_FIELDS;
        if self.edit_issue_id.is_some() && self.selected_field == 0 {
            self.selected_field = ISSUE_FORM_FIELDS - 1;
        }
    }

    pub fn cycle_current_field(&mut self, reverse: bool) {
        match self.selected_field {
            0 if self.edit_issue_id.is_none() && !self.teams.is_empty() => {
                let len = self.teams.len();
                self.team_idx = if reverse {
                    (self.team_idx + len - 1) % len
                } else {
                    (self.team_idx + 1) % len
                };
                let tid = self.teams[self.team_idx].id.clone();
                let mut states: Vec<WorkflowState> = self
                    .all_states
                    .iter()
                    .filter(|s| s.team_id == tid)
                    .cloned()
                    .collect();
                states.sort_by(|a, b| a.category.cmp(&b.category).then_with(|| a.name.cmp(&b.name)));
                self.state_idx = states
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case("Todo"))
                    .unwrap_or(0);
                self.states = states;

                let mut projects: Vec<Project> = self
                    .all_projects
                    .iter()
                    .filter(|p| {
                        p.archived_at.is_none()
                            && (p.team_ids.is_empty() || p.team_ids.contains(&tid))
                    })
                    .cloned()
                    .collect();
                projects.sort_by(|a, b| a.name.cmp(&b.name));
                self.projects = projects;
                self.project_idx = 0;
            }
            2 if !self.states.is_empty() => {
                let len = self.states.len();
                self.state_idx = if reverse {
                    (self.state_idx + len - 1) % len
                } else {
                    (self.state_idx + 1) % len
                };
            }
            3 => {
                let order = [
                    IssuePriority::NoPriority,
                    IssuePriority::Low,
                    IssuePriority::Medium,
                    IssuePriority::High,
                    IssuePriority::Urgent,
                ];
                let pos = order.iter().position(|p| *p == self.priority).unwrap_or(0);
                let next = if reverse {
                    (pos + order.len() - 1) % order.len()
                } else {
                    (pos + 1) % order.len()
                };
                self.priority = order[next];
            }
            4 => {
                let len = self.users.len() + 1;
                self.assignee_idx = if reverse {
                    (self.assignee_idx + len - 1) % len
                } else {
                    (self.assignee_idx + 1) % len
                };
            }
            5 => {
                let len = self.projects.len() + 1;
                self.project_idx = if reverse {
                    (self.project_idx + len - 1) % len
                } else {
                    (self.project_idx + 1) % len
                };
            }
            _ => {}
        }
    }

    pub fn build_mutation(&mut self) -> Option<LinearMutation> {
        if self.title_editor.text != self.title && !self.title.is_empty() && self.title_editor.is_empty() {
            self.title_editor.set_text(self.title.clone());
        } else {
            self.title = self.title_editor.text.clone();
        }
        if self.description_editor.text != self.description
            && !self.description.is_empty()
            && self.description_editor.is_empty()
        {
            self.description_editor.set_text(self.description.clone());
        } else {
            self.description = self.description_editor.text.clone();
        }
        let trimmed_title = self.title.trim();
        if trimmed_title.is_empty() {
            return None;
        }
        let state_id = self.states.get(self.state_idx).map(|s| s.id.clone());
        let assignee_id = if self.assignee_idx == 0 {
            None
        } else {
            self.users.get(self.assignee_idx - 1).map(|u| u.id.clone())
        };
        let project_id = if self.project_idx == 0 {
            None
        } else {
            self.projects
                .get(self.project_idx - 1)
                .map(|p| p.id.clone())
        };

        if let Some(ref issue_id) = self.edit_issue_id {
            let ident = self.edit_identifier.as_deref().unwrap_or("Issue");
            Some(LinearMutation::UpdateIssue {
                issue_id: issue_id.clone(),
                title: Some(CompactString::from(trimmed_title)),
                description: Some(self.description.clone()),
                state_id,
                priority: Some(self.priority),
                assignee_id: Some(assignee_id),
                project_id: Some(project_id),
                label_ids: None,
                summary_label: format!("Edited {ident}").into(),
            })
        } else {
            let team_id = self.teams.get(self.team_idx)?.id.clone();
            Some(LinearMutation::CreateIssue {
                team_id,
                title: CompactString::from(trimmed_title),
                description: Some(self.description.clone()),
                state_id,
                priority: Some(self.priority),
                assignee_id,
                project_id,
                label_ids: Vec::new(),
            })
        }
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(78, 82)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let header_title = if let Some(ref ident) = self.edit_identifier {
            format!(" Edit Issue {ident} ")
        } else {
            " Create New Linear Issue ".to_string()
        };
        let mut title_spans = vec![
            Span::styled(header_title, theme().border.title),
            Span::styled(" Ins: $EDITOR • Esc+Enter: Submit ", theme().border_title),
        ];
        if self.esc_pending {
            title_spans.push(Span::styled(
                " [ESC: Enter=save • Esc=cancel] ",
                theme().active_tab,
            ));
        }
        let title = Line::from(title_spans);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let team_str = self
            .teams
            .get(self.team_idx)
            .map(|t| format!("{} ({})", t.name, t.key))
            .unwrap_or_else(|| "None".to_string());
        let state_str = self
            .states
            .get(self.state_idx)
            .map(|s| s.name.to_string())
            .unwrap_or_else(|| "Default".to_string());
        let pri_str = self.priority.label().to_string();
        let assignee_str = if self.assignee_idx == 0 {
            "Unassigned".to_string()
        } else {
            self.users
                .get(self.assignee_idx - 1)
                .map(|u| u.display_name.to_string())
                .unwrap_or_else(|| "Unassigned".to_string())
        };
        let project_str = if self.project_idx == 0 {
            "Unassigned".to_string()
        } else {
            self.projects
                .get(self.project_idx - 1)
                .map(|p| p.name.to_string())
                .unwrap_or_else(|| "Unassigned".to_string())
        };

        let title_val = if self.selected_field == 1 {
            self.title_editor.text_with_cursor()
        } else if self.title_editor.is_empty() {
            "(required — type issue title)".to_string()
        } else {
            self.title_editor.text.clone()
        };

        let fields: [(&str, String); 6] = [
            ("Team (←/→)", team_str),
            ("Title", title_val),
            ("Workflow State (←/→)", state_str),
            ("Priority (←/→)", pri_str),
            ("Assignee (←/→)", assignee_str),
            ("Project (←/→)", project_str),
        ];

        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            "Tab / ↑ / ↓: switch field • ← / →: cycle selector • Ins: edit Description in $EDITOR • Esc+Enter: save • Esc+Esc: cancel",
            theme().input_description,
        )));
        lines.push(Line::from(""));

        for (idx, (label, val)) in fields.into_iter().enumerate() {
            let is_sel = self.selected_field == idx;
            let prefix = if is_sel { "▶ " } else { "  " };
            let label_style = if is_sel {
                Style::default()
                    .fg(Gruvbox::YellowBright.into())
                    .add_modifier(Modifier::BOLD)
            } else {
                theme().input_label
            };
            let val_style = if is_sel {
                theme().input_selected
            } else {
                theme().input
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{:<24}", label), label_style),
                Span::styled(format!(" {val} "), val_style),
            ]));
        }

        lines.push(Line::from(""));
        let desc_sel = self.selected_field == 6;
        let desc_label_style = if desc_sel {
            Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD)
        } else {
            theme().input_label
        };
        lines.push(Line::from(vec![
            Span::styled(if desc_sel { "▶ " } else { "  " }, desc_label_style),
            Span::styled(
                "Description (Markdown — press Ins to open $EDITOR, Emacs keys supported):",
                desc_label_style,
            ),
        ]));

        let desc_text = if desc_sel {
            self.description_editor.text_with_cursor()
        } else if self.description_editor.is_empty() {
            "(empty)".to_string()
        } else {
            self.description_editor.text.clone()
        };
        for dline in desc_text.split('\n').take(10) {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(dline.to_string(), theme().issue_title),
            ]));
        }

        lines.push(Line::from(""));
        let submit_sel = self.selected_field == 7;
        let submit_style = if submit_sel {
            theme().input_selected
        } else {
            theme().project_name
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if submit_sel {
                    "▶ [ Submit to Linear (Enter / Esc+Enter) ] "
                } else {
                    "  [ Submit to Linear (Esc+Enter) ] "
                },
                submit_style,
            ),
        ]));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}
