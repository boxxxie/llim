use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use tracing::Level;

use crate::{
    config::LlimConfig,
    dispatcher::Dispatcher,
    domain::{
        Comment, Issue, IssueHistoryEntry, IssueLabel, LinearMutation, Project, RateLimitStatus,
        Team, User, ViewKind, WorkflowState,
    },
    id::{ActivityId, IssueId, ProjectId},
    result::LlimError,
};

#[derive(Debug, Clone)]
pub struct BootstrapPayload {
    pub viewer: Option<User>,
    pub resolved_team_keys: Vec<CompactString>,
    pub unmatched_teams: Vec<CompactString>,
    pub teams: Vec<Team>,
    pub states: Vec<WorkflowState>,
    pub users: Vec<User>,
    pub labels: Vec<IssueLabel>,
    pub projects: Vec<Project>,
    pub issues: Vec<Issue>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DeltaBatchPayload {
    pub resolved_team_keys: Vec<CompactString>,
    pub teams: Vec<Team>,
    pub states: Vec<WorkflowState>,
    pub users: Vec<User>,
    pub labels: Vec<IssueLabel>,
    pub projects: Vec<Project>,
    pub issues: Vec<Issue>,
    pub comments_by_issue: Vec<(IssueId, Vec<Comment>)>,
    pub history_by_issue: Vec<(IssueId, Vec<IssueHistoryEntry>)>,
    pub advance_issue_watermark: Option<DateTime<Utc>>,
    pub advance_project_watermark: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct IssueActivityPayload {
    pub issue_id: IssueId,
    pub synced_updated_at: DateTime<Utc>,
    pub comments: Vec<Comment>,
    pub history: Vec<IssueHistoryEntry>,
}

#[derive(Debug, Clone)]
pub enum LlimEvent {
    // Terminal inputs & frame ticks
    InputKey(KeyEvent),
    InputMouse(MouseEvent),
    Tick,

    // Lifecycle & modal controls
    Shutdown,
    ConfigOpen,
    ConfigClose,
    ConfigApply {
        config: LlimConfig,
        persist_api_key: bool,
    },
    HelpOpen,
    HelpClose,

    // View & scope navigation
    SwitchView(ViewKind),
    FocusToggle,
    ToggleActivityFeed,
    CycleTeamScope {
        reverse: bool,
    },
    CycleProjectScope {
        reverse: bool,
    },
    CycleIssueSort,
    ToggleShowCompleted,
    ToggleActivityBatchExpand,
    FilterInputStart,
    FilterInputChar(CompactString),
    FilterInputBackspace,
    FilterInputApply(CompactString),
    FilterInputClear,

    // Cursor movement in active view/inspector
    CursorNext,
    CursorPrevious,
    CursorPageDown,
    CursorPageUp,
    CursorFirst,
    CursorLast,
    InspectorScrollDown(u16),
    InspectorScrollUp(u16),
    ToggleSelectIssue {
        move_down: bool,
    },
    ToggleSelectAllVisible,
    ClearSelection,
    EscapeOrQuit,

    // Cursor-agnostic & explicit modal actions
    OpenSelectedDetails,
    OpenSelectedProjectDetails,
    ProjectDetailsOpen(Option<ProjectId>),
    ProjectDetailsClose,
    IssueDetailsOpen(IssueId),
    IssueDetailsClose,
    ActivityDetailsOpen(ActivityId),
    ActivityDetailsClose,
    CopySelectedIssueKey,
    CopySelectedGitBranch,
    OpenSelectedUrl,
    CopyToClipboard {
        label: CompactString,
        value: CompactString,
    },
    OpenUrl(CompactString),

    // Write / mutation modals & execution
    OpenStatePicker,
    OpenPriorityPicker,
    OpenAssigneePicker,
    AssignSelectedToSelf,
    OpenProjectPicker,
    OpenLabelPicker,
    OpenCommentCompose,
    SaveCommentDraft {
        issue_id: IssueId,
        body: String,
    },
    ClearCommentDraft {
        issue_id: IssueId,
        notify: bool,
    },
    OpenEditIssueForm,
    OpenCreateIssueForm,
    WriteModalClose,
    SubmitMutation(Box<LinearMutation>),
    IssueMutated {
        issue: Box<Issue>,
        summary: CompactString,
        created: bool,
        show_notice: bool,
    },
    CommentCreated {
        issue_id: IssueId,
        comment: Comment,
        show_notice: bool,
    },
    LabelCreated(IssueLabel),
    MutationFailed(CompactString),
    LaunchExternalEditor {
        initial_text: String,
    },
    ExternalEditorCompleted(String),

    // Synchronization lifecycle & callbacks
    InitialHydrationStart,
    ManualRefresh,
    InitialHydrationLoaded(Box<BootstrapPayload>),
    DeltaBatchLoaded(Box<DeltaBatchPayload>),
    ActiveIdsReconciled {
        active_ids: Vec<(IssueId, DateTime<Utc>)>,
        reconciled_at: DateTime<Utc>,
    },
    IssueDetailsLoaded(Box<IssueActivityPayload>),
    ProjectsRefreshed(Vec<Project>),
    RateLimitUpdated(RateLimitStatus),
    SyncError(CompactString),
    AppError(LlimError),

    // Notifications & logging
    FlushNotificationBuffer,
    NotificationDismiss,
    NotificationRecallLast,
    ActivityNotice(CompactString),
    LogEntry(CompactString),
    LogLevelChanged(Level),
}

#[derive(Debug)]
pub struct EventHandler {
    sender: mpsc::Sender<LlimEvent>,
    receiver: mpsc::Receiver<LlimEvent>,
    fast_tick: Arc<AtomicBool>,
    _handler: thread::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();
        let fast_tick = Arc::new(AtomicBool::new(false));

        let handler = {
            let sender = sender.clone();
            let fast_tick = Arc::clone(&fast_tick);
            thread::spawn(move || {
                let mut last_tick = Instant::now();
                loop {
                    let current_rate = if fast_tick.load(Ordering::Relaxed) {
                        tick_rate
                    } else {
                        Duration::from_secs(15)
                    };
                    let timeout = current_rate
                        .checked_sub(last_tick.elapsed())
                        .unwrap_or(Duration::ZERO)
                        .min(Duration::from_millis(50));

                    if event::poll(timeout).unwrap_or(false) {
                        Self::apply_event(&sender);
                    }

                    if last_tick.elapsed() >= current_rate {
                        sender.dispatch(LlimEvent::Tick);
                        last_tick = Instant::now();
                    }
                }
            })
        };

        Self {
            sender,
            receiver,
            fast_tick,
            _handler: handler,
        }
    }

    pub fn set_fast_tick(&self, enabled: bool) {
        self.fast_tick.store(enabled, Ordering::Relaxed);
    }

    pub fn sender(&self) -> mpsc::Sender<LlimEvent> {
        self.sender.clone()
    }

    pub fn next(&self) -> Result<LlimEvent, mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn try_next(&self) -> Option<LlimEvent> {
        self.receiver.try_recv().ok()
    }

    fn apply_event(sender: &mpsc::Sender<LlimEvent>) {
        if let Ok(ev) = event::read() {
            match ev {
                CrosstermEvent::Key(e) if e.kind == KeyEventKind::Press => {
                    let _ = sender.send(LlimEvent::InputKey(e));
                }
                CrosstermEvent::Mouse(e)
                    if matches!(
                        e.kind,
                        MouseEventKind::Down(MouseButton::Left)
                            | MouseEventKind::Moved
                            | MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                    ) =>
                {
                    let _ = sender.send(LlimEvent::InputMouse(e));
                }
                _ => {}
            }
        }
    }
}
