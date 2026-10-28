use std::{cell::RefCell, collections::HashSet, rc::Rc};

use compact_str::CompactString;

use crate::{
    config::LlimConfig,
    domain::{Issue, Project, Team, ViewKind},
    event::LlimEvent,
    id::{IssueId, ProjectId, ProjectScope},
    input::{
        input_processor::InputProcessor,
        processor::{
            activity_details::ActivityDetailsProcessor,
            config::ConfigProcessor,
            help::HelpProcessor,
            issue_details::IssueDetailsProcessor,
            normal::NormalModeProcessor,
            project_details::ProjectDetailsProcessor,
            write::{
                CommentComposeProcessor, IssueFormProcessor, LabelPickerProcessor,
                QuickPickerProcessor,
            },
        },
    },
    notice_service::Notice,
    stores::LinearStore,
    ui::{
        popup::{
            activity_details_popup::ActivityDetailsPopup,
            config_popup::ConfigPopup,
            help_popup::HelpPopup,
            issue_details_popup::IssueDetailsPopup,
            project_details_popup::ProjectDetailsPopup,
            write_popup::{
                CommentComposePopup, IssueFormPopup, LabelPickerPopup, QuickPickerPopup,
            },
        },
        widget::{
            activity_feed::ActivityFeedState,
            attention_table::{AttentionRow, AttentionTableState},
            details_inspector::DetailsInspectorState,
            issues_table::IssuesTableState,
            shortcuts::Shortcuts,
        },
    },
};

#[derive(Debug, Default, Clone)]
pub struct FilterScope {
    pub team_key: Option<CompactString>,
    pub project_scope: ProjectScope,
    pub search_query: CompactString,
    pub filtering: bool,
    pub show_completed_issues: bool,
}

pub enum ModalState {
    IssueDetails(Rc<RefCell<IssueDetailsPopup>>),
    ProjectDetails(Rc<RefCell<ProjectDetailsPopup>>),
    ActivityDetails(Rc<RefCell<ActivityDetailsPopup>>),
    Config(Rc<RefCell<ConfigPopup>>),
    Help(Rc<RefCell<HelpPopup>>),
    QuickPicker(Rc<RefCell<QuickPickerPopup>>),
    LabelPicker(Rc<RefCell<LabelPickerPopup>>),
    CommentCompose(Rc<RefCell<CommentComposePopup>>),
    IssueForm(Rc<RefCell<IssueFormPopup>>),
}

pub struct ModalFrame {
    pub state: ModalState,
    pub processor: Box<dyn InputProcessor>,
}

pub struct StatefulWidgets {
    pub active_view: ViewKind,
    pub focus_inspector: bool,
    pub show_inspector: bool,
    pub filter_scope: FilterScope,
    pub selected_issue_ids: HashSet<IssueId>,
    pub issues_view: IssuesTableState,
    pub attention_view: AttentionTableState,
    pub activity_feed: ActivityFeedState,
    pub inspector: DetailsInspectorState,
    pub normal_processor: NormalModeProcessor,
    pub modal_stack: Vec<ModalFrame>,
    pub active_notice: Option<Notice>,
    pub last_generation: u64,
}

impl StatefulWidgets {
    pub fn new(config: &LlimConfig) -> Self {
        let default_view = config.views.default_view;
        let show_completed = config.views.show_completed_issues;
        Self {
            active_view: default_view,
            focus_inspector: false,
            show_inspector: true,
            filter_scope: FilterScope {
                team_key: None,
                project_scope: ProjectScope::All,
                search_query: CompactString::default(),
                filtering: false,
                show_completed_issues: show_completed,
            },
            selected_issue_ids: HashSet::new(),
            issues_view: IssuesTableState::new(show_completed),
            attention_view: AttentionTableState::new(),
            activity_feed: ActivityFeedState::new(),
            inspector: DetailsInspectorState::new(),
            normal_processor: NormalModeProcessor::new(default_view),
            modal_stack: Vec::new(),
            active_notice: None,
            last_generation: 0,
        }
    }

    pub fn set_focus_inspector(&mut self, focused: bool) {
        self.focus_inspector = focused && self.show_inspector;
        self.normal_processor.inspector_focused = self.focus_inspector;
    }

    pub fn toggle_focus_inspector(&mut self) {
        if self.show_inspector {
            self.set_focus_inspector(!self.focus_inspector);
        }
    }

    pub fn toggle_show_inspector(&mut self) {
        self.show_inspector = !self.show_inspector;
        if !self.show_inspector {
            self.set_focus_inspector(false);
        }
    }

    pub fn active_shortcuts(&self) -> Shortcuts {
        if let Some(top) = self.modal_stack.last() {
            top.processor.shortcuts()
        } else {
            self.normal_processor.shortcuts()
        }
    }

    pub fn dispatch_input(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let Some(top) = self.modal_stack.last_mut() {
            top.processor.apply(event, ui_events);
        } else {
            self.normal_processor.apply(event, ui_events);
        }
    }

    pub fn push_issue_details(&mut self, issue_id: &IssueId, store: &LinearStore) -> bool {
        let Some(issue) = store.issues.get(issue_id).cloned() else {
            return false;
        };
        let comments = store
            .comments_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default();
        let history = store
            .history_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default();
        let comment_draft = store.comment_draft(issue_id).map(ToString::to_string);
        let needs_load = issue.details_synced_at != Some(issue.updated_at);

        let popup = Rc::new(RefCell::new(IssueDetailsPopup::new(
            issue,
            comments,
            history,
            comment_draft,
            needs_load,
        )));
        let processor = Box::new(IssueDetailsProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::IssueDetails(popup),
            processor,
        });
        needs_load
    }

    pub fn update_open_issue_details(&mut self, issue_id: &IssueId, store: &LinearStore) {
        let Some(issue) = store.issues.get(issue_id).cloned() else {
            return;
        };
        let comments = store
            .comments_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default();
        let history = store
            .history_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default();
        let comment_draft = store.comment_draft(issue_id).map(ToString::to_string);

        for frame in &mut self.modal_stack {
            if let ModalState::IssueDetails(ref popup) = frame.state {
                if popup.borrow().issue().id == *issue_id {
                    popup.borrow_mut().update_data(
                        issue.clone(),
                        comments.clone(),
                        history.clone(),
                        comment_draft.clone(),
                    );
                }
            }
        }
    }

    pub fn push_project_details(&mut self, project_id: &ProjectId, store: &LinearStore) {
        let Some(project) = store.projects.get(project_id).cloned() else {
            return;
        };
        let mut project_issues: Vec<Issue> = store
            .issues
            .values()
            .filter(|i| i.project_id.as_ref() == Some(project_id))
            .cloned()
            .collect();
        project_issues.sort_by_key(|a| std::cmp::Reverse(a.updated_at));

        let popup = Rc::new(RefCell::new(ProjectDetailsPopup::new(
            project,
            project_issues,
        )));
        let processor = Box::new(ProjectDetailsProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::ProjectDetails(popup),
            processor,
        });
    }

    pub fn push_activity_details(
        &mut self,
        activity_id: &crate::id::ActivityId,
        store: &LinearStore,
    ) {
        let Some(activity) = store
            .activity_events
            .iter()
            .find(|ev| &ev.id == activity_id)
            .cloned()
        else {
            return;
        };
        let popup = Rc::new(RefCell::new(ActivityDetailsPopup::new(activity)));
        let processor = Box::new(ActivityDetailsProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::ActivityDetails(popup),
            processor,
        });
    }

    pub fn push_config_popup(&mut self, config: LlimConfig) {
        let popup = Rc::new(RefCell::new(ConfigPopup::new(config)));
        let processor = Box::new(ConfigProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::Config(popup),
            processor,
        });
    }

    pub fn push_help_popup(&mut self, quack_uri: Option<String>) {
        let popup = Rc::new(RefCell::new(HelpPopup::new(quack_uri)));
        let processor = Box::new(HelpProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::Help(popup),
            processor,
        });
    }

    pub fn push_quick_picker(&mut self, popup: QuickPickerPopup) {
        let popup = Rc::new(RefCell::new(popup));
        let processor = Box::new(QuickPickerProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::QuickPicker(popup),
            processor,
        });
    }

    pub fn push_label_picker(&mut self, popup: LabelPickerPopup) {
        let popup = Rc::new(RefCell::new(popup));
        let processor = Box::new(LabelPickerProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::LabelPicker(popup),
            processor,
        });
    }

    pub fn push_comment_compose(&mut self, popup: CommentComposePopup) {
        let popup = Rc::new(RefCell::new(popup));
        let processor = Box::new(CommentComposeProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::CommentCompose(popup),
            processor,
        });
    }

    pub fn push_issue_form(&mut self, popup: IssueFormPopup) {
        let popup = Rc::new(RefCell::new(popup));
        let processor = Box::new(IssueFormProcessor::new(Rc::clone(&popup)));
        self.modal_stack.push(ModalFrame {
            state: ModalState::IssueForm(popup),
            processor,
        });
    }

    pub fn apply_external_editor_text(&mut self, text: String) {
        if let Some(top) = self.modal_stack.last() {
            match &top.state {
                ModalState::CommentCompose(p) => {
                    let mut popup = p.borrow_mut();
                    popup.set_body(text);
                    popup.is_draft = !popup.body.trim().is_empty();
                }
                ModalState::IssueForm(p) => {
                    p.borrow_mut().set_description(text);
                }
                _ => {}
            }
        }
    }

    pub fn pop_modal(&mut self) -> bool {
        if let Some(frame) = self.modal_stack.pop() {
            frame.processor.on_pop();
            true
        } else {
            false
        }
    }

    pub fn selected_issue<'a>(&'a self, store: &'a LinearStore) -> Option<&'a Issue> {
        match self.active_view {
            ViewKind::Issues => self.issues_view.selected_issue(),
            ViewKind::Attention => self.attention_view.selected_issue(),
            ViewKind::Activity => {
                let issue_id = self.activity_feed.selected_row()?.issue_id();
                store.issues.get(issue_id)
            }
        }
    }

    pub fn toggle_select_current(&mut self, store: &LinearStore, move_down: bool) {
        if let Some(issue_id) = self.selected_issue(store).map(|i| i.id.clone()) {
            if !self.selected_issue_ids.remove(&issue_id) {
                self.selected_issue_ids.insert(issue_id);
            }
            if move_down {
                self.cursor_next();
            }
        }
    }

    pub fn toggle_select_all_visible(&mut self) {
        let visible_ids: Vec<IssueId> = match self.active_view {
            ViewKind::Issues => self.issues_view.rows.iter().map(|i| i.id.clone()).collect(),
            ViewKind::Attention => self
                .attention_view
                .rows
                .iter()
                .map(|r| r.issue.id.clone())
                .collect(),
            ViewKind::Activity => Vec::new(),
        };
        if visible_ids.is_empty() {
            return;
        }
        let all_selected = visible_ids
            .iter()
            .all(|id| self.selected_issue_ids.contains(id));
        if all_selected {
            for id in visible_ids {
                self.selected_issue_ids.remove(&id);
            }
        } else {
            for id in visible_ids {
                self.selected_issue_ids.insert(id);
            }
        }
    }

    pub fn clear_selection(&mut self) -> usize {
        let count = self.selected_issue_ids.len();
        self.selected_issue_ids.clear();
        count
    }

    /// Returns the multi-selected issues (in current table order) if any are selected,
    /// or a 1-element `Vec` with the focused cursor issue when no multi-selection is active.
    pub fn target_issues(&self, store: &LinearStore) -> Vec<Issue> {
        if self.selected_issue_ids.is_empty() {
            return self.selected_issue(store).cloned().into_iter().collect();
        }

        let mut ordered = Vec::with_capacity(self.selected_issue_ids.len());
        let mut seen = HashSet::with_capacity(self.selected_issue_ids.len());

        match self.active_view {
            ViewKind::Issues => {
                for row in &self.issues_view.rows {
                    if self.selected_issue_ids.contains(&row.id) && seen.insert(row.id.clone()) {
                        ordered.push(row.clone());
                    }
                }
            }
            ViewKind::Attention => {
                for row in &self.attention_view.rows {
                    if self.selected_issue_ids.contains(&row.issue.id)
                        && seen.insert(row.issue.id.clone())
                    {
                        ordered.push(row.issue.clone());
                    }
                }
            }
            ViewKind::Activity => {}
        }

        for id in &self.selected_issue_ids {
            if !seen.contains(id) {
                if let Some(iss) = store.issues.get(id) {
                    ordered.push(iss.clone());
                }
            }
        }
        ordered
    }

    /// Synchronizes the bottom 1/2 `DetailsInspectorState` with the currently selected row
    /// in the active top view, and returns `Some(IssueId)` if that issue needs Tier 0 backfill.
    pub fn sync_inspector(&mut self, store: &LinearStore) -> Option<IssueId> {
        let selected_activity = if self.active_view == ViewKind::Activity {
            self.activity_feed.selected_event().cloned()
        } else {
            None
        };

        let issue = self.selected_issue(store).cloned();
        let (comments, history, comment_draft, needs_load, promote_id) = match issue {
            Some(ref iss) => {
                let c = store
                    .comments_by_issue
                    .get(&iss.id)
                    .cloned()
                    .unwrap_or_default();
                let h = store
                    .history_by_issue
                    .get(&iss.id)
                    .cloned()
                    .unwrap_or_default();
                let d = store.comment_draft(&iss.id).map(ToString::to_string);
                let unsynced = iss.details_synced_at != Some(iss.updated_at);
                let pid = if unsynced {
                    Some(iss.id.clone())
                } else {
                    None
                };
                (c, h, d, unsynced, pid)
            }
            None => (Vec::new(), Vec::new(), None, false, None),
        };

        self.inspector.update_selection(
            issue,
            selected_activity,
            comments,
            history,
            comment_draft,
            needs_load,
        );
        promote_id
    }

    pub fn active_project<'a>(&self, store: &'a LinearStore) -> Option<&'a Project> {
        if let ProjectScope::Project(ref pid) = self.filter_scope.project_scope {
            if let Some(proj) = store.projects.get(pid) {
                return Some(proj);
            }
        }
        if let Some(issue) = self.selected_issue(store) {
            if let Some(ref pid) = issue.project_id {
                return store.projects.get(pid);
            }
        }
        None
    }

    pub fn cycle_team_scope(&mut self, store: &LinearStore, reverse: bool) {
        let mut teams: Vec<&Team> = store.teams.values().collect();
        teams.sort_by(|a, b| a.key.cmp(&b.key));

        let mut options: Vec<Option<CompactString>> = Vec::with_capacity(teams.len() + 1);
        options.push(None);
        for t in teams {
            options.push(Some(t.key.clone()));
        }

        let current_idx = options
            .iter()
            .position(|opt| opt == &self.filter_scope.team_key)
            .unwrap_or(0);
        let next_idx = if reverse {
            (current_idx + options.len() - 1) % options.len()
        } else {
            (current_idx + 1) % options.len()
        };

        self.filter_scope.team_key = options[next_idx].clone();
        if let (Some(ref tkey), ProjectScope::Project(ref pid)) =
            (&self.filter_scope.team_key, &self.filter_scope.project_scope)
        {
            let belongs = store
                .projects
                .get(pid)
                .map(|p| p.team_keys.is_empty() || p.team_keys.contains(tkey))
                .unwrap_or(false);
            if !belongs {
                self.filter_scope.project_scope = ProjectScope::All;
            }
        }
        self.recompute_projections(store);
    }

    pub fn cycle_project_scope(&mut self, store: &LinearStore, reverse: bool) {
        let mut projects: Vec<&Project> = store
            .projects
            .values()
            .filter(|p| {
                p.archived_at.is_none()
                    && match self.filter_scope.team_key {
                        Some(ref tkey) => p.team_keys.is_empty() || p.team_keys.contains(tkey),
                        None => true,
                    }
            })
            .collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));

        let mut options: Vec<ProjectScope> = Vec::with_capacity(projects.len() + 2);
        options.push(ProjectScope::All);
        options.push(ProjectScope::Unassigned);
        for p in projects {
            options.push(ProjectScope::Project(p.id.clone()));
        }

        let current_idx = options
            .iter()
            .position(|opt| opt == &self.filter_scope.project_scope)
            .unwrap_or(0);
        let next_idx = if reverse {
            (current_idx + options.len() - 1) % options.len()
        } else {
            (current_idx + 1) % options.len()
        };

        self.filter_scope.project_scope = options[next_idx].clone();
        self.recompute_projections(store);
    }

    pub fn recompute_projections(&mut self, store: &LinearStore) {
        let viewer_id = store.viewer.as_ref().map(|v| &v.id);
        let query_lower = self.filter_scope.search_query.trim().to_ascii_lowercase();

        let matches_scope = |issue: &Issue| -> bool {
            if let Some(ref tk) = self.filter_scope.team_key {
                if !issue.team_key.eq_ignore_ascii_case(tk.as_str()) {
                    return false;
                }
            }
            match self.filter_scope.project_scope {
                ProjectScope::All => {}
                ProjectScope::Unassigned => {
                    if issue.project_id.is_some() {
                        return false;
                    }
                }
                ProjectScope::Project(ref pid) => {
                    if issue.project_id.as_ref() != Some(pid) {
                        return false;
                    }
                }
            }
            if !query_lower.is_empty() {
                let id_match = issue.identifier.to_ascii_lowercase().contains(&query_lower);
                let title_match = issue.title.to_ascii_lowercase().contains(&query_lower);
                let proj_match = issue
                    .project_name
                    .as_ref()
                    .map(|p| p.to_ascii_lowercase().contains(&query_lower))
                    .unwrap_or(false);
                let assignee_match = issue
                    .assignee_name
                    .as_ref()
                    .map(|a| a.to_ascii_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                    || issue
                        .creator_name
                        .as_ref()
                        .map(|c| c.to_ascii_lowercase().contains(&query_lower))
                        .unwrap_or(false);
                let label_match = issue
                    .labels
                    .iter()
                    .any(|l| l.to_ascii_lowercase().contains(&query_lower));
                if !(id_match || title_match || proj_match || assignee_match || label_match) {
                    return false;
                }
            }
            true
        };

        let mut issue_rows = Vec::new();
        let mut attention_rows = Vec::new();

        for issue in store.issues.values() {
            if !matches_scope(issue) {
                continue;
            }
            if self.filter_scope.show_completed_issues || issue.is_active() {
                issue_rows.push(issue.clone());
            }
            let reasons = issue.attention_reasons(viewer_id);
            if !reasons.is_empty() {
                attention_rows.push(AttentionRow {
                    issue: issue.clone(),
                    reasons,
                });
            }
        }

        self.issues_view.show_completed = self.filter_scope.show_completed_issues;
        self.issues_view.set_rows(issue_rows);
        self.attention_view.set_rows(attention_rows);

        let feed_events: Vec<_> = store
            .activity_events
            .iter()
            .filter(|ev| {
                if let Some(ref tk) = self.filter_scope.team_key {
                    if !ev.team_key.eq_ignore_ascii_case(tk.as_str()) {
                        return false;
                    }
                }
                match self.filter_scope.project_scope {
                    ProjectScope::All => {}
                    ProjectScope::Unassigned => {
                        if ev.project_id.is_some() {
                            return false;
                        }
                    }
                    ProjectScope::Project(ref pid) => {
                        if ev.project_id.as_ref() != Some(pid) {
                            return false;
                        }
                    }
                }
                if !query_lower.is_empty() {
                    let id_match = ev
                        .issue_identifier
                        .to_ascii_lowercase()
                        .contains(&query_lower);
                    let title_match = ev.issue_title.to_ascii_lowercase().contains(&query_lower);
                    let actor_match = ev.actor_name.to_ascii_lowercase().contains(&query_lower);
                    let sum_match = ev.summary.to_ascii_lowercase().contains(&query_lower);
                    if !(id_match || title_match || actor_match || sum_match) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        self.activity_feed.set_events(feed_events);
        let _ = self.sync_inspector(store);
        self.last_generation = store.generation;
    }

    pub fn cursor_next(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_down(1);
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.next(),
                ViewKind::Attention => self.attention_view.next(),
                ViewKind::Activity => self.activity_feed.next(),
            }
        }
    }

    pub fn cursor_previous(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_up(1);
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.previous(),
                ViewKind::Attention => self.attention_view.previous(),
                ViewKind::Activity => self.activity_feed.previous(),
            }
        }
    }

    pub fn cursor_page_down(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_down(10);
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.page_down(),
                ViewKind::Attention => self.attention_view.page_down(),
                ViewKind::Activity => self.activity_feed.page_down(),
            }
        }
    }

    pub fn cursor_page_up(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_up(10);
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.page_up(),
                ViewKind::Attention => self.attention_view.page_up(),
                ViewKind::Activity => self.activity_feed.page_up(),
            }
        }
    }

    pub fn cursor_first(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_top();
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.first(),
                ViewKind::Attention => self.attention_view.first(),
                ViewKind::Activity => self.activity_feed.first(),
            }
        }
    }

    pub fn cursor_last(&mut self) {
        if self.focus_inspector && self.show_inspector {
            self.inspector.scroll_bottom();
        } else {
            match self.active_view {
                ViewKind::Issues => self.issues_view.last(),
                ViewKind::Attention => self.attention_view.last(),
                ViewKind::Activity => self.activity_feed.last(),
            }
        }
    }
}
