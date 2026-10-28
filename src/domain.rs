use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::{Display, Formatter},
};

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::id::{
    ActivityId, CommentId, HistoryId, IssueId, LabelId, ProjectId, StateId, TeamId, UserId,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    #[default]
    Issues,
    Attention,
    Activity,
}

impl ViewKind {
    pub fn next(self) -> Self {
        match self {
            ViewKind::Issues => ViewKind::Attention,
            ViewKind::Attention => ViewKind::Activity,
            ViewKind::Activity => ViewKind::Issues,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ViewKind::Issues => ViewKind::Activity,
            ViewKind::Attention => ViewKind::Issues,
            ViewKind::Activity => ViewKind::Attention,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSortOrder {
    #[default]
    UpdatedDesc,
    CreatedDesc,
    PriorityAndUpdated,
    StateAndPriority,
    Identifier,
}

impl IssueSortOrder {
    pub fn next(self) -> Self {
        match self {
            Self::UpdatedDesc => Self::CreatedDesc,
            Self::CreatedDesc => Self::PriorityAndUpdated,
            Self::PriorityAndUpdated => Self::StateAndPriority,
            Self::StateAndPriority => Self::Identifier,
            Self::Identifier => Self::UpdatedDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "Updated↓",
            Self::CreatedDesc => "Created↓",
            Self::PriorityAndUpdated => "Pri+Updated",
            Self::StateAndPriority => "State+Pri",
            Self::Identifier => "ID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: CompactString,
    pub display_name: CompactString,
    pub email: Option<CompactString>,
    #[serde(default)]
    pub is_bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub key: CompactString,
    pub name: CompactString,
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum StateCategory {
    Started = 0,
    Unstarted = 1,
    #[default]
    Triage = 2,
    Backlog = 3,
    Completed = 4,
    Canceled = 5,
}

impl StateCategory {
    pub fn from_linear_type(linear_type: &str) -> Self {
        match linear_type.trim().to_ascii_lowercase().as_str() {
            "started" => Self::Started,
            "unstarted" => Self::Unstarted,
            "triage" => Self::Triage,
            "backlog" => Self::Backlog,
            "completed" => Self::Completed,
            "canceled" | "cancelled" => Self::Canceled,
            _ => Self::Unstarted,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Started | Self::Unstarted | Self::Triage | Self::Backlog
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Unstarted => "unstarted",
            Self::Triage => "triage",
            Self::Backlog => "backlog",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowState {
    pub id: StateId,
    pub team_id: TeamId,
    pub name: CompactString,
    pub color: CompactString,
    pub category: StateCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueLabel {
    pub id: LabelId,
    pub team_id: Option<TeamId>,
    pub name: CompactString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: CompactString,
    pub description: CompactString,
    pub state: CompactString,
    pub progress: f64,
    pub scope: f64,
    pub target_date: Option<CompactString>,
    pub url: CompactString,
    pub team_ids: Vec<TeamId>,
    pub team_keys: Vec<CompactString>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuePriority {
    Urgent,
    High,
    Medium,
    Low,
    #[default]
    NoPriority,
}

impl IssuePriority {
    pub fn from_linear_int(value: i32) -> Self {
        match value {
            1 => Self::Urgent,
            2 => Self::High,
            3 => Self::Medium,
            4 => Self::Low,
            _ => Self::NoPriority,
        }
    }

    pub fn to_linear_int(self) -> i32 {
        match self {
            Self::Urgent => 1,
            Self::High => 2,
            Self::Medium => 3,
            Self::Low => 4,
            Self::NoPriority => 0,
        }
    }

    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Urgent => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
            Self::NoPriority => 4,
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Urgent => "URG",
            Self::High => "HIGH",
            Self::Medium => "MED",
            Self::Low => "LOW",
            Self::NoPriority => "---",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Urgent => "Urgent",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::NoPriority => "No Priority",
        }
    }
}

impl Ord for IssuePriority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_rank().cmp(&other.sort_rank())
    }
}

impl PartialOrd for IssuePriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    Mention,
    Blocked,
    Urgent,
    InProgress,
    Assigned,
    Todo,
}

impl AttentionReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mention => "@Mention",
            Self::Blocked => "Blocked",
            Self::Urgent => "Urgent",
            Self::InProgress => "In Progress",
            Self::Assigned => "Assigned",
            Self::Todo => "Todo",
        }
    }
}

impl Display for AttentionReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub identifier: CompactString,
    pub title: CompactString,
    pub description: Option<String>,
    pub team_id: TeamId,
    pub team_key: CompactString,
    pub team_name: CompactString,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<CompactString>,
    pub state_id: StateId,
    pub state_name: CompactString,
    pub state_color: CompactString,
    pub state_category: StateCategory,
    pub priority: IssuePriority,
    pub assignee_id: Option<UserId>,
    pub assignee_name: Option<CompactString>,
    pub creator_id: Option<UserId>,
    pub creator_name: Option<CompactString>,
    pub labels: Vec<CompactString>,
    #[serde(default)]
    pub label_ids: Vec<LabelId>,
    pub blocked_by: Vec<CompactString>,
    pub has_unacknowledged_mention: bool,
    pub git_branch_name: Option<CompactString>,
    pub url: CompactString,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub details_synced_at: Option<DateTime<Utc>>,
}

impl Issue {
    pub fn is_active(&self) -> bool {
        self.archived_at.is_none() && self.state_category.is_active()
    }

    /// Returns `(name, is_creator_fallback)` for display in tables and inspectors.
    /// When `assignee_name` is present, returns `(assignee, false)`.
    /// When unassigned, falls back to `creator_name` with `is_creator_fallback = true`.
    pub fn assignee_or_creator_display(&self) -> (CompactString, bool) {
        if let Some(ref assignee) = self.assignee_name {
            let trimmed = assignee.trim();
            if !trimmed.is_empty() {
                return (trimmed.into(), false);
            }
        }
        if let Some(ref creator) = self.creator_name {
            let trimmed = creator.trim();
            if !trimmed.is_empty() {
                return (format!("{trimmed} (creator)").into(), true);
            }
        }
        ("—".into(), false)
    }

    pub fn is_blocked(&self) -> bool {
        !self.blocked_by.is_empty()
            || self
                .labels
                .iter()
                .any(|l| l.eq_ignore_ascii_case("blocked") || l.eq_ignore_ascii_case("blocker"))
            || self.state_name.eq_ignore_ascii_case("blocked")
    }

    /// Computes all applicable attention reasons for View 2 (`My Attention View`).
    /// Returns an empty `Vec` if the issue is not active or not relevant to `viewer`.
    pub fn attention_reasons(&self, viewer_id: Option<&UserId>) -> Vec<AttentionReason> {
        if !self.is_active() {
            return Vec::new();
        }
        let is_assigned_to_viewer = viewer_id
            .and_then(|v| self.assignee_id.as_ref().map(|a| a == v))
            .unwrap_or(false);
        let is_relevant = self.has_unacknowledged_mention
            || is_assigned_to_viewer
            || self.priority == IssuePriority::Urgent;

        if !is_relevant {
            return Vec::new();
        }

        let mut reasons = Vec::with_capacity(4);
        if self.has_unacknowledged_mention {
            reasons.push(AttentionReason::Mention);
        }
        if self.is_blocked() {
            reasons.push(AttentionReason::Blocked);
        }
        if self.priority == IssuePriority::Urgent {
            reasons.push(AttentionReason::Urgent);
        }
        if is_assigned_to_viewer {
            match self.state_category {
                StateCategory::Started => reasons.push(AttentionReason::InProgress),
                StateCategory::Unstarted | StateCategory::Triage | StateCategory::Backlog => {
                    reasons.push(AttentionReason::Todo);
                }
                _ => reasons.push(AttentionReason::Assigned),
            }
        }
        if reasons.is_empty() && is_assigned_to_viewer {
            reasons.push(AttentionReason::Assigned);
        }
        reasons
    }

    pub fn backfill_score(&self, viewer_id: Option<&UserId>, tier_override: Option<u8>) -> BackfillScore {
        let is_assigned = viewer_id
            .and_then(|v| self.assignee_id.as_ref().map(|a| a == v))
            .unwrap_or(false);
        let tier = if let Some(explicit_tier) = tier_override {
            explicit_tier
        } else if self.is_active()
            && (is_assigned
                || self.has_unacknowledged_mention
                || matches!(self.priority, IssuePriority::Urgent | IssuePriority::High))
        {
            2
        } else if self.is_active() {
            3
        } else {
            4
        };
        BackfillScore {
            tier,
            priority_rank: self.priority.sort_rank(),
            updated_at_millis: self.updated_at.timestamp_millis(),
            issue_id: self.id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillScore {
    /// Lower tier number = higher priority (0 = open popup, 1 = live delta, 2 = active personal/urgent, 3 = other active, 4 = historical).
    pub tier: u8,
    pub priority_rank: u8,
    pub updated_at_millis: i64,
    pub issue_id: IssueId,
}

impl Ord for BackfillScore {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap, so invert tier and priority_rank (smaller is higher priority)
        other
            .tier
            .cmp(&self.tier)
            .then_with(|| other.priority_rank.cmp(&self.priority_rank))
            .then_with(|| self.updated_at_millis.cmp(&other.updated_at_millis))
            .then_with(|| other.issue_id.cmp(&self.issue_id))
    }
}

impl PartialOrd for BackfillScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillJob {
    pub issue_id: IssueId,
    pub score: BackfillScore,
}

impl Ord for BackfillJob {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

impl PartialOrd for BackfillJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub team_key: CompactString,
    pub user_id: Option<UserId>,
    pub user_name: CompactString,
    pub is_bot: bool,
    pub body: String,
    pub url: CompactString,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueHistoryEntry {
    pub id: HistoryId,
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub team_key: CompactString,
    pub actor_id: Option<UserId>,
    pub actor_name: Option<CompactString>,
    pub is_bot: bool,
    pub from_state: Option<CompactString>,
    pub to_state: Option<CompactString>,
    pub from_priority: Option<IssuePriority>,
    pub to_priority: Option<IssuePriority>,
    pub from_assignee: Option<CompactString>,
    pub to_assignee: Option<CompactString>,
    pub from_project: Option<CompactString>,
    pub to_project: Option<CompactString>,
    pub added_labels: Vec<CompactString>,
    pub removed_labels: Vec<CompactString>,
    pub updated_description: bool,
    pub created_at: DateTime<Utc>,
}

impl IssueHistoryEntry {
    pub fn summary(&self) -> CompactString {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref to_s) = self.to_state {
            if let Some(ref from_s) = self.from_state {
                parts.push(format!("State: {from_s} -> {to_s}"));
            } else {
                parts.push(format!("State -> {to_s}"));
            }
        }
        if let Some(to_p) = self.to_priority {
            if let Some(from_p) = self.from_priority {
                parts.push(format!("Priority: {} -> {}", from_p.label(), to_p.label()));
            } else {
                parts.push(format!("Priority -> {}", to_p.label()));
            }
        }
        if let Some(ref to_a) = self.to_assignee {
            parts.push(format!("Assignee -> {to_a}"));
        } else if self.from_assignee.is_some() {
            parts.push("Unassigned".to_string());
        }
        if let Some(ref to_proj) = self.to_project {
            parts.push(format!("Project -> {to_proj}"));
        }
        if !self.added_labels.is_empty() {
            parts.push(format!("+Labels: {}", self.added_labels.join(", ")));
        }
        if !self.removed_labels.is_empty() {
            parts.push(format!("-Labels: {}", self.removed_labels.join(", ")));
        }
        if self.updated_description {
            parts.push("Updated description".to_string());
        }
        if parts.is_empty() {
            "Updated issue".into()
        } else {
            parts.join(", ").into()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    IssueCreated,
    StateChanged {
        from: Option<CompactString>,
        to: CompactString,
    },
    PriorityChanged {
        from: Option<IssuePriority>,
        to: IssuePriority,
    },
    AssigneeChanged {
        to: Option<CompactString>,
    },
    Commented {
        mentioned_viewer: bool,
        excerpt: CompactString,
    },
    Updated {
        summary: CompactString,
    },
}

impl ActivityKind {
    pub fn is_direct_attention(&self, assigned_to_viewer: bool) -> bool {
        match self {
            ActivityKind::Commented {
                mentioned_viewer: true,
                ..
            } => true,
            ActivityKind::AssigneeChanged { .. } if assigned_to_viewer => true,
            ActivityKind::PriorityChanged {
                to: IssuePriority::Urgent,
                ..
            } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: ActivityId,
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub issue_title: CompactString,
    pub team_id: TeamId,
    pub team_key: CompactString,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<CompactString>,
    pub actor_id: Option<UserId>,
    pub actor_name: CompactString,
    pub is_bot: bool,
    pub kind: ActivityKind,
    pub summary: CompactString,
    pub body: Option<String>,
    pub url: CompactString,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub batch_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainChange {
    pub issue_id: IssueId,
    pub issue_identifier: CompactString,
    pub issue_title: CompactString,
    pub team_key: CompactString,
    pub project_name: Option<CompactString>,
    pub actor_id: Option<UserId>,
    pub actor_name: Option<CompactString>,
    pub is_bot: bool,
    pub is_direct_attention: bool,
    pub summary: CompactString,
    pub url: CompactString,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HydrationState {
    #[default]
    ColdBootstrap,
    CatchUpDelta,
    LiveSteadyState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncMeta {
    pub cache_schema_version: u32,
    pub synced_team_keys: Vec<CompactString>,
    pub synced_max_age_days: Option<u32>,
    pub issue_watermark: Option<DateTime<Utc>>,
    pub project_watermark: Option<DateTime<Utc>>,
    pub last_full_reconcile_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub acknowledged_mentions: HashMap<IssueId, DateTime<Utc>>,
    #[serde(default)]
    pub comment_drafts: HashMap<IssueId, String>,
}

impl Default for SyncMeta {
    fn default() -> Self {
        Self {
            cache_schema_version: 1,
            synced_team_keys: Vec::new(),
            synced_max_age_days: None,
            issue_watermark: None,
            project_watermark: None,
            last_full_reconcile_at: None,
            acknowledged_mentions: HashMap::new(),
            comment_drafts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub requests_remaining: Option<u32>,
    pub requests_limit: Option<u32>,
    pub complexity_remaining: Option<u32>,
    pub complexity_limit: Option<u32>,
    pub reset_epoch_secs: Option<i64>,
}

impl Default for RateLimitStatus {
    fn default() -> Self {
        Self {
            requests_remaining: Some(1500),
            requests_limit: Some(1500),
            complexity_remaining: Some(250_000),
            complexity_limit: Some(250_000),
            reset_epoch_secs: None,
        }
    }
}

impl RateLimitStatus {
    pub fn quota_percent(&self) -> u8 {
        let req_pct = match (self.requests_remaining, self.requests_limit) {
            (Some(rem), Some(lim)) if lim > 0 => ((rem as f64 / lim as f64) * 100.0) as u8,
            _ => 100,
        };
        let cmp_pct = match (self.complexity_remaining, self.complexity_limit) {
            (Some(rem), Some(lim)) if lim > 0 => ((rem as f64 / lim as f64) * 100.0) as u8,
            _ => 100,
        };
        req_pct.min(cmp_pct).min(100)
    }

    pub fn is_low_budget(&self) -> bool {
        self.quota_percent() < 15
    }

    pub fn allows_backfill(&self) -> bool {
        self.quota_percent() >= 30
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearMutation {
    UpdateIssue {
        issue_id: IssueId,
        title: Option<CompactString>,
        description: Option<String>,
        state_id: Option<StateId>,
        priority: Option<IssuePriority>,
        assignee_id: Option<Option<UserId>>,
        project_id: Option<Option<ProjectId>>,
        label_ids: Option<Vec<LabelId>>,
        summary_label: CompactString,
    },
    CreateIssue {
        team_id: TeamId,
        title: CompactString,
        description: Option<String>,
        state_id: Option<StateId>,
        priority: Option<IssuePriority>,
        assignee_id: Option<UserId>,
        project_id: Option<ProjectId>,
        label_ids: Vec<LabelId>,
    },
    CreateComment {
        issue_id: IssueId,
        body: String,
    },
    CreateLabelAndAttach {
        issue_id: IssueId,
        team_id: TeamId,
        label_name: CompactString,
        existing_label_ids: Vec<LabelId>,
    },
    Batch {
        mutations: Vec<LinearMutation>,
        summary_label: CompactString,
    },
}

