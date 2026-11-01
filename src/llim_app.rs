use std::{path::PathBuf, sync::mpsc::Sender, time::Instant};

use compact_str::CompactString;
use ratatui::layout::Rect;
use tachyonfx::Duration as FxDuration;
use tokio::sync::watch;

use crate::{
    client::poller::LinearPollerHandle,
    config::{LlimConfig, save_config},
    dispatcher::Dispatcher,
    domain::{HydrationState, LinearMutation, RateLimitStatus, ViewKind},
    effect_registry::EffectRegistry,
    event::LlimEvent,
    input::processor::issue_details::slugify,
    notice_service::{NoticeLevel, NoticeMessage, NoticeService},
    notification_buffer::NotificationBuffer,
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
        stateful_widgets::StatefulWidgets,
        widget::notification::NotificationBanner,
    },
};

pub struct LlimApp {
    pub running: bool,
    pub config: LlimConfig,
    pub config_path: PathBuf,
    pub config_tx: watch::Sender<LlimConfig>,
    pub store: LinearStore,
    pub widgets: StatefulWidgets,
    pub notices: NoticeService,
    pub notification_buffer: NotificationBuffer,
    pub effects: EffectRegistry,
    pub poller: Option<LinearPollerHandle>,
    pub sender: Sender<LlimEvent>,
    pub rate_limit: RateLimitStatus,
    pub offline: bool,
    pub last_frame_instant: Instant,
    pub terminal_area: Rect,
    pub pending_external_editor: Option<String>,
}

impl LlimApp {
    pub fn new(
        config: LlimConfig,
        config_path: PathBuf,
        config_tx: watch::Sender<LlimConfig>,
        store: LinearStore,
        poller: Option<LinearPollerHandle>,
        sender: Sender<LlimEvent>,
        offline: bool,
    ) -> Self {
        let mut widgets = StatefulWidgets::new(&config);
        widgets.recompute_projections(&store);
        let effects = EffectRegistry::new(sender.clone(), config.effects.clone());

        let mut app = Self {
            running: true,
            config,
            config_path,
            config_tx,
            store,
            widgets,
            notices: NoticeService::new(),
            notification_buffer: NotificationBuffer::new(),
            effects,
            poller,
            sender,
            rate_limit: RateLimitStatus::default(),
            offline,
            last_frame_instant: Instant::now(),
            terminal_area: Rect::new(0, 0, 120, 40),
            pending_external_editor: None,
        };
        app.sync_inspector_and_backfill();
        app
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn elapsed_fx_duration(&mut self) -> FxDuration {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_instant);
        self.last_frame_instant = now;
        FxDuration::from_millis(elapsed.as_millis().min(100) as u32)
    }

    fn sync_inspector_and_backfill(&mut self) {
        if let Some(promote_id) = self.widgets.sync_inspector(&self.store) {
            if let Some(ref poller) = self.poller {
                poller.promote_issue(promote_id);
            }
        }
    }

    pub fn apply(&mut self, event: LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        self.notices.apply(&event);

        match event {
            LlimEvent::InputKey(_) | LlimEvent::InputMouse(_) => {
                self.widgets.dispatch_input(&event, ui_events);
            }
            LlimEvent::Tick => {}
            LlimEvent::Shutdown => {
                self.store.shutdown();
                self.running = false;
            }
            LlimEvent::ConfigOpen => {
                self.widgets.push_config_popup(self.config.clone());
                self.effects
                    .register_popup_open(ConfigPopup::popup_area(self.terminal_area));
            }
            LlimEvent::ConfigClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(ConfigPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::ConfigApply {
                config,
                persist_api_key,
            } => {
                self.widgets.pop_modal();
                self.effects
                    .register_popup_close(ConfigPopup::popup_area(self.terminal_area));

                if let Err(err) = save_config(&self.config_path, &config, persist_api_key) {
                    self.sender.dispatch(LlimEvent::AppError(err));
                } else {
                    self.config = config.clone();
                    self.effects.effects_config = config.effects.clone();
                    self.widgets.filter_scope.show_completed_issues =
                        config.views.show_completed_issues;
                    self.widgets.recompute_projections(&self.store);
                    self.sync_inspector_and_backfill();
                    let _ = self.config_tx.send(config);
                    self.sender
                        .dispatch(LlimEvent::ActivityNotice("Configuration saved".into()));
                }
            }
            LlimEvent::HelpOpen => {
                let quack_uri = self.store.quack_uri().map(ToString::to_string);
                self.widgets.push_help_popup(quack_uri);
                self.effects
                    .register_popup_open(HelpPopup::popup_area(self.terminal_area));
            }
            LlimEvent::HelpClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(HelpPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::SwitchView(view) => {
                self.widgets.active_view = view;
                self.widgets.normal_processor.active_view = view;
                self.widgets.set_focus_inspector(false);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::FocusToggle => {
                if let Some(issue_id) = self
                    .widgets
                    .selected_issue(&self.store)
                    .map(|i| i.id.clone())
                {
                    self.store.acknowledge_issue_mention(&issue_id);
                    self.widgets.recompute_projections(&self.store);
                }
                if !self.widgets.show_inspector {
                    self.widgets.show_inspector = true;
                }
                self.widgets.toggle_focus_inspector();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::OpenSelectedDetails => {
                if let Some(issue_id) = self
                    .widgets
                    .selected_issue(&self.store)
                    .map(|i| i.id.clone())
                {
                    ui_events.push(LlimEvent::IssueDetailsOpen(issue_id));
                } else if self.widgets.active_view == ViewKind::Activity {
                    if let Some(act_id) = self
                        .widgets
                        .activity_feed
                        .selected_event()
                        .map(|a| a.id.clone())
                    {
                        ui_events.push(LlimEvent::ActivityDetailsOpen(act_id));
                    }
                }
            }
            LlimEvent::ToggleActivityFeed => {
                self.widgets.toggle_show_inspector();
            }
            LlimEvent::CycleTeamScope { reverse } => {
                self.widgets.cycle_team_scope(&self.store, reverse);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CycleProjectScope { reverse } => {
                self.widgets.cycle_project_scope(&self.store, reverse);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CycleIssueSort => {
                self.widgets.issues_view.cycle_sort();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::ToggleShowCompleted => {
                self.widgets.filter_scope.show_completed_issues =
                    !self.widgets.filter_scope.show_completed_issues;
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::ToggleActivityBatchExpand => {
                self.widgets.activity_feed.toggle_selected_batch();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::ToggleSelectIssue { move_down } => {
                self.widgets.toggle_select_current(&self.store, move_down);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::ToggleSelectAllVisible => {
                self.widgets.toggle_select_all_visible();
            }
            LlimEvent::ClearSelection => {
                self.widgets.clear_selection();
            }
            LlimEvent::EscapeOrQuit => {
                if !self.widgets.selected_issue_ids.is_empty() {
                    let count = self.widgets.clear_selection();
                    self.sender.dispatch(LlimEvent::ActivityNotice(
                        format!("Cleared {count} selected issues").into(),
                    ));
                } else {
                    ui_events.push(LlimEvent::Shutdown);
                }
            }
            LlimEvent::FilterInputStart => {
                self.widgets.filter_scope.filtering = true;
            }
            LlimEvent::FilterInputChar(_) | LlimEvent::FilterInputBackspace => {
                self.widgets.filter_scope.search_query =
                    self.widgets.normal_processor.filter_text.clone();
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::FilterInputApply(query) => {
                self.widgets.filter_scope.filtering = false;
                self.widgets.filter_scope.search_query = query;
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::FilterInputClear => {
                self.widgets.filter_scope.filtering = false;
                self.widgets.filter_scope.search_query.clear();
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorNext => {
                self.widgets.cursor_next();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorPrevious => {
                self.widgets.cursor_previous();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorPageDown => {
                self.widgets.cursor_page_down();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorPageUp => {
                self.widgets.cursor_page_up();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorFirst => {
                self.widgets.cursor_first();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::CursorLast => {
                self.widgets.cursor_last();
                self.sync_inspector_and_backfill();
            }
            LlimEvent::InspectorScrollDown(amount) => {
                self.widgets.inspector.scroll_down(amount);
            }
            LlimEvent::InspectorScrollUp(amount) => {
                self.widgets.inspector.scroll_up(amount);
            }
            LlimEvent::OpenSelectedProjectDetails => {
                let pid = self
                    .widgets
                    .active_project(&self.store)
                    .map(|p| p.id.clone());
                ui_events.push(LlimEvent::ProjectDetailsOpen(pid));
            }
            LlimEvent::ProjectDetailsOpen(opt_pid) => {
                if let Some(pid) = opt_pid {
                    self.widgets.push_project_details(&pid, &self.store);
                    self.effects
                        .register_popup_open(ProjectDetailsPopup::popup_area(self.terminal_area));
                } else {
                    self.sender.dispatch(LlimEvent::ActivityNotice(
                        "Selected issue has no assigned project".into(),
                    ));
                }
            }
            LlimEvent::ProjectDetailsClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(ProjectDetailsPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::IssueDetailsOpen(issue_id) => {
                self.store.acknowledge_issue_mention(&issue_id);
                let needs_load = self.widgets.push_issue_details(&issue_id, &self.store);
                self.widgets.recompute_projections(&self.store);
                self.effects
                    .register_popup_open(IssueDetailsPopup::popup_area(self.terminal_area));
                if needs_load {
                    if let Some(ref poller) = self.poller {
                        poller.promote_issue(issue_id);
                    }
                }
            }
            LlimEvent::IssueDetailsClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(IssueDetailsPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::IssueDetailsNext | LlimEvent::IssueDetailsPrev => {
                let next = matches!(event, LlimEvent::IssueDetailsNext);
                self.widgets.advance_list_cursor(next);
                self.sync_inspector_and_backfill();
                if let Some(issue_id) = self
                    .widgets
                    .selected_list_issue(&self.store)
                    .map(|i| i.id.clone())
                {
                    self.store.acknowledge_issue_mention(&issue_id);
                    let needs_load = self.widgets.push_issue_details(&issue_id, &self.store);
                    self.widgets.recompute_projections(&self.store);
                    if needs_load {
                        if let Some(ref poller) = self.poller {
                            poller.promote_issue(issue_id);
                        }
                    }
                }
            }
            LlimEvent::ActivityDetailsOpen(activity_id) => {
                self.widgets
                    .push_activity_details(&activity_id, &self.store);
                self.effects
                    .register_popup_open(ActivityDetailsPopup::popup_area(self.terminal_area));
            }
            LlimEvent::ActivityDetailsClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(ActivityDetailsPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::CopySelectedIssueKey => {
                let targets = self.widgets.target_issues(&self.store);
                if let [single] = targets.as_slice() {
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: "Issue ID".into(),
                        value: single.identifier.clone(),
                    });
                } else if !targets.is_empty() {
                    let joined: CompactString = targets
                        .iter()
                        .map(|i| i.identifier.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .into();
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: format!("{} Issue IDs", targets.len()).into(),
                        value: joined,
                    });
                }
            }
            LlimEvent::CopySelectedGitBranch => {
                if let Some(issue) = self.widgets.selected_issue(&self.store) {
                    let branch = issue.git_branch_name.clone().unwrap_or_else(|| {
                        format!(
                            "feat/{}-{}",
                            issue.identifier.to_ascii_lowercase(),
                            slugify(&issue.title)
                        )
                        .into()
                    });
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: "Git branch".into(),
                        value: branch,
                    });
                }
            }
            LlimEvent::OpenSelectedUrl => {
                if self.widgets.active_view == ViewKind::Activity {
                    if let Some(ev) = self.widgets.activity_feed.selected_event() {
                        if !ev.url.is_empty() {
                            ui_events.push(LlimEvent::OpenUrl(ev.url.clone()));
                            return;
                        }
                    }
                }
                if let Some(issue) = self.widgets.selected_issue(&self.store) {
                    if !issue.url.is_empty() {
                        ui_events.push(LlimEvent::OpenUrl(issue.url.clone()));
                    }
                }
            }
            LlimEvent::CopyToClipboard { label, value } => {
                copy_text_to_clipboard(&value);
                self.sender.dispatch(LlimEvent::ActivityNotice(
                    format!("Copied {label}: {value}").into(),
                ));
            }
            LlimEvent::OpenUrl(url) => {
                let _ = open::that(url.as_str());
                self.sender.dispatch(LlimEvent::ActivityNotice(
                    format!("Opened in browser: {url}").into(),
                ));
            }
            LlimEvent::OpenStatePicker => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let popup = QuickPickerPopup::new_state_picker_for_targets(targets, &self.store);
                    self.widgets.push_quick_picker(popup);
                    self.effects
                        .register_popup_open(QuickPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::OpenPriorityPicker => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let popup = QuickPickerPopup::new_priority_picker_for_targets(targets);
                    self.widgets.push_quick_picker(popup);
                    self.effects
                        .register_popup_open(QuickPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::OpenAssigneePicker => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let popup =
                        QuickPickerPopup::new_assignee_picker_for_targets(targets, &self.store);
                    self.widgets.push_quick_picker(popup);
                    self.effects
                        .register_popup_open(QuickPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::AssignSelectedToSelf => {
                let targets = self.widgets.target_issues(&self.store);
                if targets.is_empty() {
                    return;
                }
                if let Some(ref viewer) = self.store.viewer {
                    if let [issue] = targets.as_slice() {
                        if issue.assignee_id.as_ref() != Some(&viewer.id) {
                            let mutation = LinearMutation::UpdateIssue {
                                issue_id: issue.id.clone(),
                                title: None,
                                description: None,
                                state_id: None,
                                priority: None,
                                assignee_id: Some(Some(viewer.id.clone())),
                                project_id: None,
                                label_ids: None,
                                summary_label: format!(
                                    "{}: Assignee -> {}",
                                    issue.identifier, viewer.display_name
                                )
                                .into(),
                            };
                            ui_events.push(LlimEvent::SubmitMutation(Box::new(mutation)));
                            return;
                        }
                    } else {
                        let count = targets.len();
                        let mutations = targets
                            .iter()
                            .map(|issue| LinearMutation::UpdateIssue {
                                issue_id: issue.id.clone(),
                                title: None,
                                description: None,
                                state_id: None,
                                priority: None,
                                assignee_id: Some(Some(viewer.id.clone())),
                                project_id: None,
                                label_ids: None,
                                summary_label: format!(
                                    "{}: Assignee -> {}",
                                    issue.identifier, viewer.display_name
                                )
                                .into(),
                            })
                            .collect();
                        let batch = LinearMutation::Batch {
                            mutations,
                            summary_label: format!(
                                "Assigned {count} issues -> {}",
                                viewer.display_name
                            )
                            .into(),
                        };
                        ui_events.push(LlimEvent::SubmitMutation(Box::new(batch)));
                        return;
                    }
                }
                let popup = QuickPickerPopup::new_assignee_picker_for_targets(targets, &self.store);
                self.widgets.push_quick_picker(popup);
                self.effects
                    .register_popup_open(QuickPickerPopup::popup_area(self.terminal_area));
            }
            LlimEvent::OpenProjectPicker => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let popup =
                        QuickPickerPopup::new_project_picker_for_targets(targets, &self.store);
                    self.widgets.push_quick_picker(popup);
                    self.effects
                        .register_popup_open(QuickPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::OpenLabelPicker => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let popup = LabelPickerPopup::new_for_targets(targets, &self.store);
                    self.widgets.push_label_picker(popup);
                    self.effects
                        .register_popup_open(LabelPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::OpenCommentCompose => {
                let targets = self.widgets.target_issues(&self.store);
                if !targets.is_empty() {
                    let initial_draft = targets
                        .first()
                        .and_then(|iss| self.store.comment_draft(&iss.id));
                    let popup =
                        CommentComposePopup::new_for_targets_with_draft(targets, initial_draft);
                    self.widgets.push_comment_compose(popup);
                    self.effects
                        .register_popup_open(CommentComposePopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::SaveCommentDraft { issue_id, body } => {
                self.store.set_comment_draft(issue_id.clone(), body);
                self.widgets.update_open_issue_details(&issue_id, &self.store);
                self.sync_inspector_and_backfill();
            }
            LlimEvent::ClearCommentDraft { issue_id, notify } => {
                let had_draft = self.store.clear_comment_draft(&issue_id);
                self.widgets.update_open_issue_details(&issue_id, &self.store);
                self.sync_inspector_and_backfill();
                if notify && had_draft {
                    let ident = self
                        .store
                        .issues
                        .get(&issue_id)
                        .map(|i| i.identifier.as_str())
                        .unwrap_or("issue");
                    self.sender.dispatch(LlimEvent::ActivityNotice(
                        format!("Deleted comment draft for {ident}").into(),
                    ));
                }
            }
            LlimEvent::OpenEditIssueForm => {
                if let Some(issue) = self.widgets.selected_issue(&self.store).cloned() {
                    let popup = IssueFormPopup::new_edit(&issue, &self.store);
                    self.widgets.push_issue_form(popup);
                    self.effects
                        .register_popup_open(IssueFormPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::OpenCreateIssueForm => {
                let active_tk = self.widgets.filter_scope.team_key.as_deref();
                let popup = IssueFormPopup::new_create(&self.store, active_tk);
                self.widgets.push_issue_form(popup);
                self.effects
                    .register_popup_open(IssueFormPopup::popup_area(self.terminal_area));
            }
            LlimEvent::WriteModalClose => {
                if self.widgets.pop_modal() {
                    self.effects
                        .register_popup_close(QuickPickerPopup::popup_area(self.terminal_area));
                }
            }
            LlimEvent::SubmitMutation(mutation) => {
                if matches!(*mutation, LinearMutation::Batch { .. }) {
                    self.widgets.clear_selection();
                }
                if self.offline {
                    self.sender.dispatch(LlimEvent::MutationFailed(
                        "Cannot mutate Linear issues while in --offline mode".into(),
                    ));
                } else if let Some(ref poller) = self.poller {
                    poller.execute_mutation(*mutation);
                }
            }
            LlimEvent::IssueMutated {
                issue,
                summary,
                created,
                show_notice,
            } => {
                let issue_id = issue.id.clone();
                self.store.apply_mutated_issue(*issue, &summary, created);
                let dirty = self.store.take_dirty_projects();
                if !dirty.is_empty() {
                    if let Some(ref poller) = self.poller {
                        poller.refresh_projects(dirty);
                    }
                }
                self.widgets.recompute_projections(&self.store);
                if created {
                    if let Some(pos) = self
                        .widgets
                        .issues_view
                        .rows
                        .iter()
                        .position(|r| r.id == issue_id)
                    {
                        self.widgets.issues_view.table_state.select(Some(pos));
                    }
                }
                self.sync_inspector_and_backfill();
                if show_notice {
                    self.sender.dispatch(LlimEvent::ActivityNotice(summary));
                }
            }
            LlimEvent::CommentCreated {
                issue_id,
                comment,
                show_notice,
            } => {
                self.store.clear_comment_draft(&issue_id);
                self.store.apply_created_comment(&issue_id, comment);
                self.widgets.update_open_issue_details(&issue_id, &self.store);
                let ident = self
                    .store
                    .issues
                    .get(&issue_id)
                    .map(|i| i.identifier.as_str())
                    .unwrap_or("issue")
                    .to_string();
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
                if show_notice {
                    self.sender.dispatch(LlimEvent::ActivityNotice(
                        format!("Posted comment on {ident}").into(),
                    ));
                }
            }
            LlimEvent::LabelCreated(label) => {
                self.store.apply_created_label(label);
            }
            LlimEvent::LaunchExternalEditor { initial_text } => {
                self.pending_external_editor = Some(initial_text);
            }
            LlimEvent::ExternalEditorCompleted(text) => {
                self.widgets.apply_external_editor_text(text.clone());
                if let Some(top) = self.widgets.modal_stack.last() {
                    if let crate::ui::stateful_widgets::ModalState::CommentCompose(ref p) =
                        top.state
                    {
                        for target in &p.borrow().targets {
                            self.store.set_comment_draft(target.id.clone(), text.clone());
                        }
                        self.sync_inspector_and_backfill();
                    }
                }
            }
            LlimEvent::InitialHydrationStart | LlimEvent::ManualRefresh => {
                if let Some(ref poller) = self.poller {
                    poller.sync_now();
                }
            }
            LlimEvent::InitialHydrationLoaded(payload) => {
                let count = payload.issues.len();
                let _unsynced = self
                    .store
                    .apply_bootstrap(*payload, self.config.polling.max_age_days);
                self.widgets.recompute_projections(&self.store);
                self.sync_inspector_and_backfill();
                self.sender.dispatch(LlimEvent::ActivityNotice(
                    format!("Hydrated {count} issues into DuckDB 2.0").into(),
                ));
            }
            LlimEvent::DeltaBatchLoaded(payload) => {
                let was_catch_up = self.store.hydration_state == HydrationState::CatchUpDelta;
                let (changes, mutated_count) = self.store.apply_delta_batch(
                    *payload,
                    self.config.polling.max_activity_events,
                    self.config.polling.max_stored_issues,
                );
                if was_catch_up {
                    self.store.hydration_state = HydrationState::LiveSteadyState;
                    if mutated_count > 0 {
                        self.sender.dispatch(LlimEvent::ActivityNotice(
                            format!("Caught up {mutated_count} updated issues since last session")
                                .into(),
                        ));
                    }
                } else if !changes.is_empty() {
                    let viewer_id = self.store.viewer.as_ref().map(|v| &v.id);
                    self.notification_buffer.enqueue(
                        changes,
                        viewer_id,
                        &self.config.notifications,
                    );
                }

                let dirty = self.store.take_dirty_projects();
                if !dirty.is_empty() {
                    if let Some(ref poller) = self.poller {
                        poller.refresh_projects(dirty);
                    }
                }

                if self.store.generation != self.widgets.last_generation {
                    self.widgets.recompute_projections(&self.store);
                    self.sync_inspector_and_backfill();
                }
            }
            LlimEvent::ActiveIdsReconciled {
                active_ids,
                reconciled_at,
            } => {
                let missing = self.store.apply_reconciled_ids(&active_ids, reconciled_at);
                if !missing.is_empty() {
                    if let Some(ref poller) = self.poller {
                        poller.sync_now();
                    }
                }
                if self.store.generation != self.widgets.last_generation {
                    self.widgets.recompute_projections(&self.store);
                    self.sync_inspector_and_backfill();
                }
            }
            LlimEvent::IssueDetailsLoaded(payload) => {
                let issue_id = payload.issue_id.clone();
                self.store.apply_issue_activity(*payload);
                self.widgets.update_open_issue_details(&issue_id, &self.store);
                self.widgets.recompute_projections(&self.store);
            }
            LlimEvent::ProjectsRefreshed(projects) => {
                self.store.apply_refreshed_projects(projects);
                self.widgets.recompute_projections(&self.store);
            }
            LlimEvent::RateLimitUpdated(status) => {
                self.rate_limit = status;
            }
            LlimEvent::FlushNotificationBuffer => {
                let alerts = self
                    .notification_buffer
                    .evaluate_and_flush(&self.config.notifications, !self.offline);
                for alert in alerts {
                    self.notices.push_notice(
                        NoticeLevel::Info,
                        NoticeMessage::GeneralMessage(
                            format!("{}: {}", alert.summary, alert.body).into(),
                        ),
                    );
                }
            }
            LlimEvent::NotificationDismiss => {
                self.widgets.active_notice = None;
                self.widgets.normal_processor.set_has_notification(false);
            }
            LlimEvent::NotificationRecallLast => {
                if let Some(last) = self.notices.last_notification().cloned() {
                    self.widgets.active_notice = Some(last);
                    self.widgets.normal_processor.set_has_notification(true);
                    self.effects.register_notification_effect(
                        NotificationBanner::banner_area(self.terminal_area),
                    );
                }
            }
            LlimEvent::SyncError(_)
            | LlimEvent::MutationFailed(_)
            | LlimEvent::AppError(_)
            | LlimEvent::ActivityNotice(_)
            | LlimEvent::LogEntry(_)
            | LlimEvent::LogLevelChanged(_) => {}
        }

        if self.widgets.active_notice.is_none() {
            if let Some(next_notice) = self.notices.pop_notice() {
                self.widgets.active_notice = Some(next_notice);
                self.widgets.normal_processor.set_has_notification(true);
                self.effects.register_notification_effect(
                    NotificationBanner::banner_area(self.terminal_area),
                );
            }
        }
    }
}

fn copy_text_to_clipboard(text: &CompactString) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if cb.set_text(text.as_str()).is_ok() {
            return;
        }
    }
    for (bin, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
    ] {
        if let Ok(mut child) = std::process::Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
}
