use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Sender},
    thread,
};

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::{
    config::{default_quack_token_path, DatabaseSection},
    domain::{
        ActivityEvent, ActivityKind, Comment, DomainChange, HydrationState, Issue,
        IssueHistoryEntry, IssueLabel, Project, StateCategory, SyncMeta, Team, User, WorkflowState,
    },
    event::{BootstrapPayload, DeltaBatchPayload, IssueActivityPayload, LlimEvent},
    id::{ActivityId, HistoryId, IssueId, LabelId, ProjectId, StateId, TeamId, UserId},
};

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS teams (
    id VARCHAR PRIMARY KEY,
    key VARCHAR,
    name VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS states (
    id VARCHAR PRIMARY KEY,
    team_id VARCHAR,
    name VARCHAR,
    color VARCHAR,
    category VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS projects (
    id VARCHAR PRIMARY KEY,
    name VARCHAR,
    state VARCHAR,
    progress DOUBLE,
    scope DOUBLE,
    updated_at VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS issues (
    id VARCHAR PRIMARY KEY,
    identifier VARCHAR,
    team_key VARCHAR,
    team_name VARCHAR,
    project_id VARCHAR,
    project_name VARCHAR,
    state_name VARCHAR,
    state_category VARCHAR,
    priority VARCHAR,
    assignee_name VARCHAR,
    title VARCHAR,
    description VARCHAR,
    git_branch_name VARCHAR,
    url VARCHAR,
    updated_at VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS comments (
    id VARCHAR PRIMARY KEY,
    issue_id VARCHAR,
    issue_identifier VARCHAR,
    team_key VARCHAR,
    user_name VARCHAR,
    is_bot BOOLEAN,
    body VARCHAR,
    created_at VARCHAR,
    updated_at VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS issue_history (
    id VARCHAR PRIMARY KEY,
    issue_id VARCHAR,
    issue_identifier VARCHAR,
    team_key VARCHAR,
    actor_name VARCHAR,
    is_bot BOOLEAN,
    to_state VARCHAR,
    to_priority VARCHAR,
    to_assignee VARCHAR,
    created_at VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS activity_events (
    id VARCHAR PRIMARY KEY,
    issue_id VARCHAR,
    issue_identifier VARCHAR,
    team_key VARCHAR,
    actor_name VARCHAR,
    summary VARCHAR,
    timestamp VARCHAR,
    payload VARCHAR
);
CREATE TABLE IF NOT EXISTS sync_meta (
    id INTEGER PRIMARY KEY,
    payload VARCHAR
);
"#;

pub fn resolve_duckdb_binary() -> PathBuf {
    if let Some(base) = directories::BaseDirs::new() {
        let home_duckdb = base.home_dir().join(".local").join("bin").join("duckdb");
        if home_duckdb.exists() {
            return home_duckdb;
        }
    }
    PathBuf::from("duckdb")
}

fn sql_escape(s: &str) -> String {
    s.replace('\'', "''").replace(['\r', '\n'], " ")
}

fn sql_opt_str(opt: Option<&str>) -> String {
    match opt {
        Some(s) => format!("'{}'", sql_escape(s)),
        None => "NULL".to_string(),
    }
}

#[derive(Debug)]
enum SqlWorkerMsg {
    Execute(String),
    Shutdown(mpsc::SyncSender<()>),
}

/// Asynchronous handle to the persistent DuckDB 2.0 engine process running `quack_serve`.
#[derive(Debug)]
pub struct DuckDbHandle {
    sql_tx: Sender<SqlWorkerMsg>,
    pub quack_listen_uri: Option<CompactString>,
    pub quack_token: Option<CompactString>,
}

impl DuckDbHandle {
    pub fn execute_async(&self, sql_batch: String) {
        if !sql_batch.trim().is_empty() {
            let _ = self.sql_tx.send(SqlWorkerMsg::Execute(sql_batch));
        }
    }

    pub fn shutdown(&self) {
        if let Some(ref uri) = self.quack_listen_uri {
            let _ = self.sql_tx.send(SqlWorkerMsg::Execute(format!(
                "CALL quack_stop('{}');",
                sql_escape(uri)
            )));
        }
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.sql_tx.send(SqlWorkerMsg::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(5));
        }
    }
}

///Normalized in-memory store backed by DuckDB 2.0 (`~/.local/share/llim/llim.duckdb`) and the Quack protocol.
pub struct LinearStore {
    pub viewer: Option<User>,
    pub teams: HashMap<TeamId, Team>,
    pub states: HashMap<StateId, WorkflowState>,
    pub users: HashMap<UserId, User>,
    pub labels: HashMap<LabelId, IssueLabel>,
    pub projects: HashMap<ProjectId, Project>,
    pub issues: HashMap<IssueId, Issue>,
    pub comments_by_issue: HashMap<IssueId, Vec<Comment>>,
    pub history_by_issue: HashMap<IssueId, Vec<IssueHistoryEntry>>,
    pub activity_events: VecDeque<ActivityEvent>,
    pub sync_meta: SyncMeta,
    pub hydration_state: HydrationState,
    pub generation: u64,
    pub batch_counter: u64,
    pub dirty_projects: HashSet<ProjectId>,
    db_path: PathBuf,
    duckdb_handle: Option<DuckDbHandle>,
}

impl LinearStore {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            viewer: None,
            teams: HashMap::new(),
            states: HashMap::new(),
            users: HashMap::new(),
            labels: HashMap::new(),
            projects: HashMap::new(),
            issues: HashMap::new(),
            comments_by_issue: HashMap::new(),
            history_by_issue: HashMap::new(),
            activity_events: VecDeque::new(),
            sync_meta: SyncMeta::default(),
            hydration_state: HydrationState::ColdBootstrap,
            generation: 1,
            batch_counter: 1,
            dirty_projects: HashSet::new(),
            db_path,
            duckdb_handle: None,
        }
    }

    pub fn quack_uri(&self) -> Option<&str> {
        self.duckdb_handle
            .as_ref()
            .and_then(|h| h.quack_listen_uri.as_deref())
    }

    /// Loads persisted entities from `~/.local/share/llim/llim.duckdb` on startup and starts
    /// the persistent DuckDB 2.0 worker with `quack_serve`.
    pub fn init_duckdb_and_load(
        &mut self,
        db_config: &DatabaseSection,
        event_sender: Option<&Sender<LlimEvent>>,
    ) {
        if let Some(parent) = self.db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let duckdb_bin = resolve_duckdb_binary();

        // 1. Ensure schema exists and load existing rows into memory in one fast batch
        if self.db_path.exists() {
            self.load_existing_rows(&duckdb_bin);
        } else {
            let _ = Command::new(&duckdb_bin)
                .arg("-dark-mode")
                .arg("-batch")
                .arg(&self.db_path)
                .arg("-c")
                .arg(SCHEMA_SQL)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        // 2. Spawn persistent DuckDB 2.0 background worker with optional Quack protocol server
        self.start_persistent_worker(&duckdb_bin, db_config, event_sender);
    }

    fn load_existing_rows(&mut self, duckdb_bin: &Path) {
        let _ = Command::new(duckdb_bin)
            .arg("-dark-mode")
            .arg("-batch")
            .arg(&self.db_path)
            .arg("-c")
            .arg(SCHEMA_SQL)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        for team in query_payloads::<Team>(duckdb_bin, &self.db_path, "SELECT payload FROM teams;") {
            self.teams.insert(team.id.clone(), team);
        }
        for state in query_payloads::<WorkflowState>(
            duckdb_bin,
            &self.db_path,
            "SELECT payload FROM states;",
        ) {
            self.states.insert(state.id.clone(), state);
        }
        for proj in
            query_payloads::<Project>(duckdb_bin, &self.db_path, "SELECT payload FROM projects;")
        {
            self.projects.insert(proj.id.clone(), proj);
        }
        for issue in
            query_payloads::<Issue>(duckdb_bin, &self.db_path, "SELECT payload FROM issues;")
        {
            self.issues.insert(issue.id.clone(), issue);
        }
        for comment in query_payloads::<Comment>(
            duckdb_bin,
            &self.db_path,
            "SELECT payload FROM comments ORDER BY created_at ASC;",
        ) {
            self.comments_by_issue
                .entry(comment.issue_id.clone())
                .or_default()
                .push(comment);
        }
        for entry in query_payloads::<IssueHistoryEntry>(
            duckdb_bin,
            &self.db_path,
            "SELECT payload FROM issue_history ORDER BY created_at ASC;",
        ) {
            self.history_by_issue
                .entry(entry.issue_id.clone())
                .or_default()
                .push(entry);
        }
        for ev in query_payloads::<ActivityEvent>(
            duckdb_bin,
            &self.db_path,
            "SELECT payload FROM activity_events ORDER BY timestamp DESC;",
        ) {
            self.activity_events.push_back(ev);
        }
        if let Some(meta) = query_payloads::<SyncMeta>(
            duckdb_bin,
            &self.db_path,
            "SELECT payload FROM sync_meta WHERE id = 1;",
        )
        .into_iter()
        .next()
        {
            self.sync_meta = meta;
        }

        // Recompute has_unacknowledged_mention from loaded comments & sync_meta
        self.recompute_all_mention_flags();
        self.harvest_users_and_labels_from_issues();
        self.generation = self.generation.wrapping_add(1);

        info!(
            teams = self.teams.len(),
            projects = self.projects.len(),
            issues = self.issues.len(),
            activity_events = self.activity_events.len(),
            "Loaded cached state from DuckDB 2.0"
        );
    }

    fn start_persistent_worker(
        &mut self,
        duckdb_bin: &Path,
        db_config: &DatabaseSection,
        event_sender: Option<&Sender<LlimEvent>>,
    ) {
        let mut child: Child = match Command::new(duckdb_bin)
            .arg("-dark-mode")
            .arg("-batch")
            .arg(&self.db_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "Failed to spawn persistent DuckDB 2.0 worker");
                return;
            }
        };

        let mut stdin: ChildStdin = match child.stdin.take() {
            Some(s) => s,
            None => return,
        };
        let stdout = child.stdout.take();

        let _ = writeln!(stdin, "{SCHEMA_SQL}");

        let mut quack_listen_uri = None;
        let mut quack_token = None;

        if db_config.quack_server_enabled {
            let uri = format!("quack:127.0.0.1:{}", db_config.quack_port);
            let token = load_or_create_quack_token();
            let init_quack_sql = format!(
                "INSTALL quack;\nLOAD quack;\nCALL quack_serve('{}', token => '{}');\nSELECT 'LLIM_QUACK_DONE';\n",
                sql_escape(&uri),
                sql_escape(&token)
            );
            if writeln!(stdin, "{init_quack_sql}").is_ok() && stdin.flush().is_ok() {
                if let Some(out) = stdout {
                    let mut reader = BufReader::new(out);
                    let mut line = String::new();
                    let mut started_ok = false;
                    while let Ok(n) = reader.read_line(&mut line) {
                        if n == 0 {
                            break;
                        }
                        if line.contains(&uri) {
                            started_ok = true;
                        }
                        if line.contains("LLIM_QUACK_DONE") {
                            break;
                        }
                        line.clear();
                    }
                    // Keep draining stdout in a background thread so pipe buffer never fills
                    thread::spawn(move || {
                        let mut sink = String::new();
                        while let Ok(n) = reader.read_line(&mut sink) {
                            if n == 0 {
                                break;
                            }
                            sink.clear();
                        }
                    });

                    if started_ok {
                        info!(uri = %uri, "Started DuckDB 2.0 Quack protocol server");
                        quack_listen_uri = Some(uri.into());
                        quack_token = Some(token);
                    } else if let Some(tx) = event_sender {
                        let _ = tx.send(LlimEvent::ActivityNotice(
                            format!(
                                "Quack port {} in use; running DuckDB in local-only mode",
                                db_config.quack_port
                            )
                            .into(),
                        ));
                    }
                }
            }
        } else if let Some(out) = stdout {
            thread::spawn(move || {
                let mut reader = BufReader::new(out);
                let mut sink = String::new();
                while let Ok(n) = reader.read_line(&mut sink) {
                    if n == 0 {
                        break;
                    }
                    sink.clear();
                }
            });
        }

        let (sql_tx, sql_rx) = mpsc::channel::<SqlWorkerMsg>();
        thread::spawn(move || {
            let mut ack_on_exit = None;
            while let Ok(msg) = sql_rx.recv() {
                match msg {
                    SqlWorkerMsg::Execute(sql) => {
                        if writeln!(stdin, "{sql}").is_err() {
                            break;
                        }
                        let _ = stdin.flush();
                    }
                    SqlWorkerMsg::Shutdown(ack) => {
                        let _ = writeln!(stdin, "CHECKPOINT;");
                        let _ = stdin.flush();
                        ack_on_exit = Some(ack);
                        break;
                    }
                }
            }
            drop(stdin);
            let _ = child.wait();
            if let Some(ack) = ack_on_exit {
                let _ = ack.send(());
            }
        });

        self.duckdb_handle = Some(DuckDbHandle {
            sql_tx,
            quack_listen_uri,
            quack_token,
        });
    }

    pub fn shutdown(&self) {
        if let Some(ref handle) = self.duckdb_handle {
            handle.shutdown();
        }
    }

    pub fn needs_cold_bootstrap(
        &self,
        resolved_team_keys: &[CompactString],
        max_age_days: Option<u32>,
    ) -> bool {
        if self.sync_meta.issue_watermark.is_none() || self.issues.is_empty() {
            return true;
        }
        if self.sync_meta.synced_max_age_days != max_age_days {
            return true;
        }
        let mut stored_teams = self.sync_meta.synced_team_keys.clone();
        stored_teams.sort();
        let mut requested_teams = resolved_team_keys.to_vec();
        requested_teams.sort();
        stored_teams != requested_teams
    }

    /// Applies initial cold-bootstrap hydration and persists all rows to DuckDB 2.0.
    pub fn apply_bootstrap(
        &mut self,
        payload: BootstrapPayload,
        max_age_days: Option<u32>,
    ) -> Vec<IssueId> {
        let mut sql = String::from("BEGIN TRANSACTION;\n");

        if let Some(viewer) = payload.viewer {
            self.viewer = Some(viewer);
        }

        // Evict entities belonging to teams no longer in resolved_team_keys
        let allowed_keys: HashSet<&str> =
            payload.resolved_team_keys.iter().map(|k| k.as_str()).collect();
        self.issues
            .retain(|_, issue| allowed_keys.contains(issue.team_key.as_str()));

        for team in payload.teams {
            sql.push_str(&upsert_team_sql(&team));
            self.teams.insert(team.id.clone(), team);
        }

        for state in payload.states {
            sql.push_str(&upsert_state_sql(&state));
            self.states.insert(state.id.clone(), state);
        }

        for user in payload.users {
            self.users.insert(user.id.clone(), user);
        }

        for label in payload.labels {
            self.labels.insert(label.id.clone(), label);
        }

        for project in payload.projects {
            sql.push_str(&upsert_project_sql(&project));
            self.projects.insert(project.id.clone(), project);
        }

        let mut unsynced_issue_ids = Vec::new();
        for mut issue in payload.issues {
            if let Some(ack_at) = self.sync_meta.acknowledged_mentions.get(&issue.id) {
                if issue.updated_at <= *ack_at {
                    issue.has_unacknowledged_mention = false;
                }
            }
            if issue.details_synced_at != Some(issue.updated_at) {
                unsynced_issue_ids.push(issue.id.clone());
            }
            sql.push_str(&upsert_issue_sql(&issue));
            self.issues.insert(issue.id.clone(), issue);
        }

        // Seed initial activity feed if empty from the 25 most recently updated issues
        if self.activity_events.is_empty() {
            let batch_id = self.next_batch_id();
            let mut recent_issues: Vec<Issue> = self.issues.values().cloned().collect();
            recent_issues.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
            for issue in recent_issues.into_iter().take(25) {
                let ev = ActivityEvent {
                    id: ActivityId::new(format!("boot-{}-{}", issue.id, issue.updated_at.timestamp())),
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_id: issue.team_id.clone(),
                    team_key: issue.team_key.clone(),
                    project_id: issue.project_id.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: issue.assignee_id.clone().or_else(|| issue.creator_id.clone()),
                    actor_name: issue
                        .assignee_name
                        .clone()
                        .or_else(|| issue.creator_name.clone())
                        .unwrap_or_else(|| "Linear".into()),
                    is_bot: false,
                    kind: ActivityKind::Updated {
                        summary: format!("State: {}", issue.state_name).into(),
                    },
                    summary: format!("State: {} ({})", issue.state_name, issue.priority.short_label())
                        .into(),
                    body: issue.description.clone(),
                    url: issue.url.clone(),
                    timestamp: issue.updated_at,
                    batch_id,
                };
                sql.push_str(&upsert_activity_sql(&ev));
                self.activity_events.push_back(ev);
            }
        }

        let watermark = payload.server_time - chrono::Duration::seconds(5);
        self.sync_meta.synced_team_keys = payload.resolved_team_keys;
        self.sync_meta.synced_max_age_days = max_age_days;
        self.sync_meta.issue_watermark = Some(watermark);
        self.sync_meta.project_watermark = Some(watermark);
        self.sync_meta.last_full_reconcile_at = Some(payload.server_time);
        sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
        sql.push_str("COMMIT;\n");

        self.persist_sql(sql);
        self.harvest_users_and_labels_from_issues();
        self.hydration_state = HydrationState::LiveSteadyState;
        self.generation = self.generation.wrapping_add(1);
        unsynced_issue_ids
    }

    /// Applies an incremental delta batch, updates DuckDB 2.0, and returns `Vec<DomainChange>`
    /// (plus the count of changed issues for `CatchUpDelta` summary notices).
    pub fn apply_delta_batch(
        &mut self,
        payload: DeltaBatchPayload,
        max_activity_events: Option<usize>,
        max_stored_issues: Option<usize>,
    ) -> (Vec<DomainChange>, usize) {
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        let mut changes = Vec::new();
        let mut mutated_issues_count = 0usize;
        let batch_id = self.next_batch_id();

        let mut parent_metadata_changed = false;
        for team in payload.teams {
            if self.teams.get(&team.id) != Some(&team) {
                parent_metadata_changed = true;
                sql.push_str(&upsert_team_sql(&team));
                self.teams.insert(team.id.clone(), team);
            }
        }

        for state in payload.states {
            if self.states.get(&state.id) != Some(&state) {
                parent_metadata_changed = true;
                sql.push_str(&upsert_state_sql(&state));
                self.states.insert(state.id.clone(), state);
            }
        }

        for user in payload.users {
            self.users.insert(user.id.clone(), user);
        }

        for label in payload.labels {
            self.labels.insert(label.id.clone(), label);
        }

        for project in payload.projects {
            let changed = self
                .projects
                .get(&project.id)
                .map(|old| old.updated_at < project.updated_at || old.name != project.name)
                .unwrap_or(true);
            if changed {
                parent_metadata_changed = true;
                sql.push_str(&upsert_project_sql(&project));
                self.projects.insert(project.id.clone(), project);
            }
        }

        // Index incoming comments and history by IssueId
        let mut incoming_comments: HashMap<IssueId, Vec<Comment>> =
            payload.comments_by_issue.into_iter().collect();
        let mut incoming_history: HashMap<IssueId, Vec<IssueHistoryEntry>> =
            payload.history_by_issue.into_iter().collect();

        let viewer_id = self.viewer.as_ref().map(|v| v.id.clone());
        let viewer_name = self.viewer.as_ref().map(|v| v.name.clone());
        let viewer_display = self.viewer.as_ref().map(|v| v.display_name.clone());

        for mut issue in payload.issues {
            // If archived or deleted on server, evict from active views or mark archived
            let prior_issue = self.issues.get(&issue.id).cloned();
            let is_strictly_newer = prior_issue
                .as_ref()
                .map(|old| issue.updated_at > old.updated_at)
                .unwrap_or(true);

            // Process new comments for this issue
            let mut new_comments_found = Vec::new();
            if let Some(comments) = incoming_comments.remove(&issue.id) {
                let existing = self.comments_by_issue.entry(issue.id.clone()).or_default();
                for comment in comments {
                    if let Some(pos) = existing.iter().position(|c| c.id == comment.id) {
                        if comment.updated_at > existing[pos].updated_at {
                            sql.push_str(&upsert_comment_sql(&comment));
                            existing[pos] = comment.clone();
                            new_comments_found.push(comment);
                        }
                    } else {
                        sql.push_str(&upsert_comment_sql(&comment));
                        existing.push(comment.clone());
                        new_comments_found.push(comment);
                    }
                }
                existing.sort_by_key(|a| a.created_at);
            }

            // Process new history entries for this issue
            let mut new_history_found = Vec::new();
            if let Some(mut entries) = incoming_history.remove(&issue.id) {
                for entry in &mut entries {
                    self.resolve_history_names(entry);
                }
                let existing = self.history_by_issue.entry(issue.id.clone()).or_default();
                let seen_ids: HashSet<HistoryId> = existing.iter().map(|h| h.id.clone()).collect();
                for entry in entries {
                    if !seen_ids.contains(&entry.id) {
                        sql.push_str(&upsert_history_sql(&entry));
                        existing.push(entry.clone());
                        new_history_found.push(entry);
                    }
                }
                existing.sort_by_key(|a| a.created_at);
            }

            if !is_strictly_newer && new_comments_found.is_empty() && new_history_found.is_empty() {
                continue;
            }

            mutated_issues_count += 1;

            // Track dirty project if state or project_id changed
            if let Some(ref old) = prior_issue {
                if old.state_id != issue.state_id || old.project_id != issue.project_id {
                    if let Some(ref pid) = old.project_id {
                        self.dirty_projects.insert(pid.clone());
                    }
                    if let Some(ref pid) = issue.project_id {
                        self.dirty_projects.insert(pid.clone());
                    }
                }
            } else if let Some(ref pid) = issue.project_id {
                self.dirty_projects.insert(pid.clone());
            }

            // Emit ActivityEvents and DomainChanges for new comments
            for comment in &new_comments_found {
                let mentioned = comment_mentions_viewer(
                    &comment.body,
                    viewer_name.as_deref(),
                    viewer_display.as_deref(),
                );
                if mentioned {
                    let ack_after = self
                        .sync_meta
                        .acknowledged_mentions
                        .get(&issue.id)
                        .map(|ack| comment.created_at <= *ack)
                        .unwrap_or(false);
                    if !ack_after {
                        issue.has_unacknowledged_mention = true;
                    }
                }
                let excerpt: CompactString = comment
                    .body
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .into();
                let summary: CompactString = if mentioned {
                    format!("@mentioned you: {excerpt}").into()
                } else {
                    format!("Commented: {excerpt}").into()
                };

                let act_ev = ActivityEvent {
                    id: ActivityId::new(format!("cmt-{}", comment.id)),
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_id: issue.team_id.clone(),
                    team_key: issue.team_key.clone(),
                    project_id: issue.project_id.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: comment.user_id.clone(),
                    actor_name: comment.user_name.clone(),
                    is_bot: comment.is_bot,
                    kind: ActivityKind::Commented {
                        mentioned_viewer: mentioned,
                        excerpt,
                    },
                    summary: summary.clone(),
                    body: Some(comment.body.clone()),
                    url: comment.url.clone(),
                    timestamp: comment.created_at,
                    batch_id,
                };
                sql.push_str(&upsert_activity_sql(&act_ev));
                self.activity_events.push_front(act_ev);

                changes.push(DomainChange {
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_key: issue.team_key.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: comment.user_id.clone(),
                    actor_name: Some(comment.user_name.clone()),
                    is_bot: comment.is_bot,
                    is_direct_attention: mentioned,
                    summary,
                    url: comment.url.clone(),
                    timestamp: comment.created_at,
                });
            }

            // Emit ActivityEvents and DomainChanges for new history entries
            let assigned_to_viewer = viewer_id
                .as_ref()
                .and_then(|v| issue.assignee_id.as_ref().map(|a| a == v))
                .unwrap_or(false);

            for entry in &new_history_found {
                let summary = entry.summary();
                let is_direct = (entry.to_assignee.is_some() && assigned_to_viewer)
                    || entry.to_priority == Some(crate::domain::IssuePriority::Urgent)
                    || entry
                        .added_labels
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case("blocked") || l.eq_ignore_ascii_case("blocker"));

                let kind = if let Some(ref to_s) = entry.to_state {
                    ActivityKind::StateChanged {
                        from: entry.from_state.clone(),
                        to: to_s.clone(),
                    }
                } else if let Some(to_p) = entry.to_priority {
                    ActivityKind::PriorityChanged {
                        from: entry.from_priority,
                        to: to_p,
                    }
                } else if entry.to_assignee.is_some() || entry.from_assignee.is_some() {
                    ActivityKind::AssigneeChanged {
                        to: entry.to_assignee.clone(),
                    }
                } else {
                    ActivityKind::Updated {
                        summary: summary.clone(),
                    }
                };

                let act_ev = ActivityEvent {
                    id: ActivityId::new(format!("hist-{}", entry.id)),
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_id: issue.team_id.clone(),
                    team_key: issue.team_key.clone(),
                    project_id: issue.project_id.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: entry.actor_id.clone(),
                    actor_name: entry
                        .actor_name
                        .clone()
                        .unwrap_or_else(|| "Linear".into()),
                    is_bot: entry.is_bot,
                    kind,
                    summary: summary.clone(),
                    body: issue.description.clone(),
                    url: issue.url.clone(),
                    timestamp: entry.created_at,
                    batch_id,
                };
                sql.push_str(&upsert_activity_sql(&act_ev));
                self.activity_events.push_front(act_ev);

                changes.push(DomainChange {
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_key: issue.team_key.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: entry.actor_id.clone(),
                    actor_name: entry.actor_name.clone(),
                    is_bot: entry.is_bot,
                    is_direct_attention: is_direct,
                    summary,
                    url: issue.url.clone(),
                    timestamp: entry.created_at,
                });
            }

            // Fallback scalar diff if an issue was newly created or updated with no history/comment nodes
            if new_comments_found.is_empty() && new_history_found.is_empty() && is_strictly_newer {
                let (kind, summary, actor_id, actor_name, is_direct) = match prior_issue {
                    None => (
                        ActivityKind::IssueCreated,
                        CompactString::from("Created issue"),
                        issue.creator_id.clone(),
                        issue.creator_name.clone(),
                        assigned_to_viewer
                            || issue.priority == crate::domain::IssuePriority::Urgent,
                    ),
                    Some(old) => {
                        let mut diffs = Vec::new();
                        if old.state_name != issue.state_name {
                            diffs.push(format!("State: {} -> {}", old.state_name, issue.state_name));
                        }
                        if old.priority != issue.priority {
                            diffs.push(format!(
                                "Priority: {} -> {}",
                                old.priority.label(),
                                issue.priority.label()
                            ));
                        }
                        if old.assignee_name != issue.assignee_name {
                            diffs.push(format!(
                                "Assignee -> {}",
                                issue.assignee_name.as_deref().unwrap_or("Unassigned")
                            ));
                        }
                        let sum: CompactString = if diffs.is_empty() {
                            "Updated metadata".into()
                        } else {
                            diffs.join(", ").into()
                        };
                        (
                            ActivityKind::Updated {
                                summary: sum.clone(),
                            },
                            sum,
                            None, // Silent diff without explicit history actor: log to feed, no OS popup
                            None,
                            false,
                        )
                    }
                };

                let act_ev = ActivityEvent {
                    id: ActivityId::new(format!(
                        "diff-{}-{}",
                        issue.id,
                        issue.updated_at.timestamp_millis()
                    )),
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_id: issue.team_id.clone(),
                    team_key: issue.team_key.clone(),
                    project_id: issue.project_id.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id: actor_id.clone(),
                    actor_name: actor_name.clone().unwrap_or_else(|| "Linear".into()),
                    is_bot: false,
                    kind,
                    summary: summary.clone(),
                    body: issue.description.clone(),
                    url: issue.url.clone(),
                    timestamp: issue.updated_at,
                    batch_id,
                };
                sql.push_str(&upsert_activity_sql(&act_ev));
                self.activity_events.push_front(act_ev);

                changes.push(DomainChange {
                    issue_id: issue.id.clone(),
                    issue_identifier: issue.identifier.clone(),
                    issue_title: issue.title.clone(),
                    team_key: issue.team_key.clone(),
                    project_name: issue.project_name.clone(),
                    actor_id,
                    actor_name,
                    is_bot: false,
                    is_direct_attention: is_direct,
                    summary,
                    url: issue.url.clone(),
                    timestamp: issue.updated_at,
                });
            }

            issue.details_synced_at = Some(issue.updated_at);
            sql.push_str(&upsert_issue_sql(&issue));
            self.issues.insert(issue.id.clone(), issue);
        }

        if parent_metadata_changed {
            self.refresh_denormalized_issue_fields(&mut sql);
        }

        if let Some(max_ev) = max_activity_events {
            while self.activity_events.len() > max_ev {
                self.activity_events.pop_back();
            }
        }

        if let Some(max_issues) = max_stored_issues {
            if self.issues.len() > max_issues {
                let mut by_age: Vec<(IssueId, bool, DateTime<Utc>)> = self
                    .issues
                    .values()
                    .map(|i| (i.id.clone(), i.is_active(), i.updated_at))
                    .collect();
                // Evict oldest inactive issues first
                by_age.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));
                let to_remove = self.issues.len().saturating_sub(max_issues);
                for (id, _, _) in by_age.into_iter().take(to_remove) {
                    self.issues.remove(&id);
                    sql.push_str(&format!(
                        "DELETE FROM issues WHERE id = '{}';\n",
                        sql_escape(id.as_str())
                    ));
                }
            }
        }

        if let Some(iw) = payload.advance_issue_watermark {
            let next_iw = match self.sync_meta.issue_watermark {
                Some(prev) if prev > iw => prev,
                _ => iw,
            };
            self.sync_meta.issue_watermark = Some(next_iw);
        }
        if let Some(pw) = payload.advance_project_watermark {
            let next_pw = match self.sync_meta.project_watermark {
                Some(prev) if prev > pw => prev,
                _ => pw,
            };
            self.sync_meta.project_watermark = Some(next_pw);
        }

        sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);
        self.harvest_users_and_labels_from_issues();

        if mutated_issues_count > 0 || parent_metadata_changed {
            self.generation = self.generation.wrapping_add(1);
        }

        (changes, mutated_issues_count)
    }

    /// Applies backfilled comments and history for an issue from `BackfillPriorityQueue` or `IssueDetailsPopup`.
    pub fn apply_issue_activity(&mut self, payload: IssueActivityPayload) {
        let mut sql = String::from("BEGIN TRANSACTION;\n");

        let viewer_name = self.viewer.as_ref().map(|v| v.name.clone());
        let viewer_display = self.viewer.as_ref().map(|v| v.display_name.clone());

        let existing_comments = self
            .comments_by_issue
            .entry(payload.issue_id.clone())
            .or_default();
        for comment in payload.comments {
            sql.push_str(&upsert_comment_sql(&comment));
            if let Some(pos) = existing_comments.iter().position(|c| c.id == comment.id) {
                existing_comments[pos] = comment;
            } else {
                existing_comments.push(comment);
            }
        }
        existing_comments.sort_by_key(|a| a.created_at);

        let mut resolved_entries = payload.history;
        for entry in &mut resolved_entries {
            self.resolve_history_names(entry);
            sql.push_str(&upsert_history_sql(entry));
        }

        let existing_history = self
            .history_by_issue
            .entry(payload.issue_id.clone())
            .or_default();
        for entry in resolved_entries {
            if let Some(pos) = existing_history.iter().position(|h| h.id == entry.id) {
                existing_history[pos] = entry;
            } else {
                existing_history.push(entry);
            }
        }
        existing_history.sort_by_key(|a| a.created_at);

        if let Some(issue) = self.issues.get_mut(&payload.issue_id) {
            issue.details_synced_at = Some(payload.synced_updated_at);
            let ack_at = self
                .sync_meta
                .acknowledged_mentions
                .get(&payload.issue_id)
                .copied();
            if let Some(comments) = self.comments_by_issue.get(&payload.issue_id) {
                issue.has_unacknowledged_mention = comments.iter().any(|c| {
                    let is_after_ack = ack_at.map(|a| c.created_at > a).unwrap_or(true);
                    is_after_ack
                        && comment_mentions_viewer(
                            &c.body,
                            viewer_name.as_deref(),
                            viewer_display.as_deref(),
                        )
                });
            }
            sql.push_str(&upsert_issue_sql(issue));
        }

        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Reconciles local issues against the full set of remote UUIDs, evicting deleted/transferred issues
    /// and returning any remote `IssueId`s that are missing locally or have a newer `updatedAt`.
    pub fn apply_reconciled_ids(
        &mut self,
        active_ids: &[(IssueId, DateTime<Utc>)],
        reconciled_at: DateTime<Utc>,
    ) -> Vec<IssueId> {
        let remote_map: HashMap<&IssueId, DateTime<Utc>> =
            active_ids.iter().map(|(id, ts)| (id, *ts)).collect();

        let to_evict: Vec<IssueId> = self
            .issues
            .keys()
            .filter(|id| !remote_map.contains_key(id))
            .cloned()
            .collect();

        let mut missing_or_stale = Vec::new();
        for (remote_id, remote_updated) in active_ids {
            match self.issues.get(remote_id) {
                None => missing_or_stale.push(remote_id.clone()),
                Some(local) if *remote_updated > local.updated_at => {
                    missing_or_stale.push(remote_id.clone());
                }
                _ => {}
            }
        }

        let mut sql = String::from("BEGIN TRANSACTION;\n");
        for id in &to_evict {
            self.issues.remove(id);
            self.comments_by_issue.remove(id);
            self.history_by_issue.remove(id);
            let escaped = sql_escape(id.as_str());
            sql.push_str(&format!("DELETE FROM issues WHERE id = '{escaped}';\n"));
            sql.push_str(&format!(
                "DELETE FROM comments WHERE issue_id = '{escaped}';\n"
            ));
            sql.push_str(&format!(
                "DELETE FROM issue_history WHERE issue_id = '{escaped}';\n"
            ));
        }

        self.sync_meta.last_full_reconcile_at = Some(reconciled_at);
        sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);

        if !to_evict.is_empty() {
            self.generation = self.generation.wrapping_add(1);
        }

        missing_or_stale
    }

    pub fn apply_refreshed_projects(&mut self, projects: Vec<Project>) {
        if projects.is_empty() {
            return;
        }
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        for project in projects {
            sql.push_str(&upsert_project_sql(&project));
            self.projects.insert(project.id.clone(), project);
        }
        self.refresh_denormalized_issue_fields(&mut sql);
        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn acknowledge_issue_mention(&mut self, issue_id: &IssueId) {
        let now = Utc::now();
        self.sync_meta
            .acknowledged_mentions
            .insert(issue_id.clone(), now);
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        if let Some(issue) = self.issues.get_mut(issue_id) {
            if issue.has_unacknowledged_mention {
                issue.has_unacknowledged_mention = false;
                sql.push_str(&upsert_issue_sql(issue));
                self.generation = self.generation.wrapping_add(1);
            }
        }
        sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);
    }

    pub fn comment_draft(&self, issue_id: &IssueId) -> Option<&str> {
        self.sync_meta
            .comment_drafts
            .get(issue_id)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
    }

    pub fn set_comment_draft(&mut self, issue_id: IssueId, body: String) {
        let changed = if body.trim().is_empty() {
            self.sync_meta.comment_drafts.remove(&issue_id).is_some()
        } else {
            self.sync_meta.comment_drafts.insert(issue_id, body)
                .map(|old| !old.is_empty())
                .unwrap_or(true)
        };
        if changed {
            let mut sql = String::from("BEGIN TRANSACTION;\n");
            sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
            sql.push_str("COMMIT;\n");
            self.persist_sql(sql);
        }
    }

    pub fn clear_comment_draft(&mut self, issue_id: &IssueId) -> bool {
        let removed = self.sync_meta.comment_drafts.remove(issue_id).is_some();
        if removed {
            let mut sql = String::from("BEGIN TRANSACTION;\n");
            sql.push_str(&upsert_sync_meta_sql(&self.sync_meta));
            sql.push_str("COMMIT;\n");
            self.persist_sql(sql);
        }
        removed
    }

    pub fn take_dirty_projects(&mut self) -> Vec<ProjectId> {
        self.dirty_projects.drain().collect()
    }

    pub fn harvest_users_and_labels_from_issues(&mut self) {
        if let Some(ref v) = self.viewer {
            self.users.insert(v.id.clone(), v.clone());
        }
        for issue in self.issues.values() {
            self.states
                .entry(issue.state_id.clone())
                .or_insert_with(|| WorkflowState {
                    id: issue.state_id.clone(),
                    team_id: issue.team_id.clone(),
                    name: issue.state_name.clone(),
                    color: issue.state_color.clone(),
                    category: issue.state_category,
                });
            if let (Some(uid), Some(uname)) = (&issue.assignee_id, &issue.assignee_name) {
                self.users.entry(uid.clone()).or_insert_with(|| User {
                    id: uid.clone(),
                    name: uname.clone(),
                    display_name: uname.clone(),
                    email: None,
                    is_bot: false,
                });
            }
            if let (Some(cid), Some(cname)) = (&issue.creator_id, &issue.creator_name) {
                self.users.entry(cid.clone()).or_insert_with(|| User {
                    id: cid.clone(),
                    name: cname.clone(),
                    display_name: cname.clone(),
                    email: None,
                    is_bot: false,
                });
            }
            for (idx, lname) in issue.labels.iter().enumerate() {
                let lid = issue
                    .label_ids
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| LabelId::new(format!("name:{lname}")));
                self.labels.entry(lid.clone()).or_insert_with(|| IssueLabel {
                    id: lid,
                    team_id: Some(issue.team_id.clone()),
                    name: lname.clone(),
                });
            }
        }
    }

    pub fn states_for_team(&self, team_id: &TeamId) -> Vec<WorkflowState> {
        let mut out: Vec<WorkflowState> = self
            .states
            .values()
            .filter(|s| &s.team_id == team_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            workflow_state_sort_key(a)
                .cmp(&workflow_state_sort_key(b))
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    pub fn projects_for_team(&self, team_id: &TeamId) -> Vec<Project> {
        let mut out: Vec<Project> = self
            .projects
            .values()
            .filter(|p| {
                p.archived_at.is_none()
                    && (p.team_ids.is_empty() || p.team_ids.contains(team_id))
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn labels_for_team(&self, team_id: &TeamId) -> Vec<IssueLabel> {
        let mut out: Vec<IssueLabel> = self
            .labels
            .values()
            .filter(|l| {
                !l.id.as_str().starts_with("name:")
                    && (l.team_id.is_none() || l.team_id.as_ref() == Some(team_id))
            })
            .cloned()
            .collect();
        out.sort_by_key(|a| a.name.to_ascii_lowercase());
        out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        out
    }

    pub fn sorted_users(&self) -> Vec<User> {
        let viewer_id = self.viewer.as_ref().map(|v| &v.id);
        let mut out: Vec<User> = self.users.values().filter(|u| !u.is_bot).cloned().collect();
        out.sort_by(|a, b| {
            let a_is_me = viewer_id == Some(&a.id);
            let b_is_me = viewer_id == Some(&b.id);
            b_is_me
                .cmp(&a_is_me)
                .then_with(|| a.display_name.cmp(&b.display_name))
        });
        out
    }

    pub fn apply_mutated_issue(&mut self, mut issue: Issue, summary: &str, created: bool) {
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        if let Some(old) = self.issues.get(&issue.id) {
            if old.state_id != issue.state_id || old.project_id != issue.project_id {
                if let Some(ref pid) = old.project_id {
                    self.dirty_projects.insert(pid.clone());
                }
                if let Some(ref pid) = issue.project_id {
                    self.dirty_projects.insert(pid.clone());
                }
            }
            issue.has_unacknowledged_mention = old.has_unacknowledged_mention;
        } else if let Some(ref pid) = issue.project_id {
            self.dirty_projects.insert(pid.clone());
        }
        issue.details_synced_at = Some(issue.updated_at);

        let batch_id = self.next_batch_id();
        let actor_id = self.viewer.as_ref().map(|v| v.id.clone());
        let actor_name = self
            .viewer
            .as_ref()
            .map(|v| v.display_name.clone())
            .unwrap_or_else(|| "You".into());

        let act_ev = ActivityEvent {
            id: ActivityId::new(format!(
                "mut-{}-{}",
                issue.id,
                Utc::now().timestamp_millis()
            )),
            issue_id: issue.id.clone(),
            issue_identifier: issue.identifier.clone(),
            issue_title: issue.title.clone(),
            team_id: issue.team_id.clone(),
            team_key: issue.team_key.clone(),
            project_id: issue.project_id.clone(),
            project_name: issue.project_name.clone(),
            actor_id,
            actor_name,
            is_bot: false,
            kind: if created {
                ActivityKind::IssueCreated
            } else {
                ActivityKind::Updated {
                    summary: summary.into(),
                }
            },
            summary: summary.into(),
            body: issue.description.clone(),
            url: issue.url.clone(),
            timestamp: issue.updated_at,
            batch_id,
        };

        sql.push_str(&upsert_issue_sql(&issue));
        sql.push_str(&upsert_activity_sql(&act_ev));
        sql.push_str("COMMIT;\n");

        self.issues.insert(issue.id.clone(), issue);
        self.activity_events.push_front(act_ev);
        self.harvest_users_and_labels_from_issues();
        self.persist_sql(sql);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn apply_created_comment(&mut self, issue_id: &IssueId, mut comment: Comment) {
        let mut sql = String::from("BEGIN TRANSACTION;\n");

        if let Some(issue) = self.issues.get_mut(issue_id) {
            if comment.issue_identifier.is_empty() {
                comment.issue_identifier = issue.identifier.clone();
            }
            if comment.team_key.is_empty() {
                comment.team_key = issue.team_key.clone();
            }
            if comment.url.is_empty() {
                comment.url = issue.url.clone();
            }
            if comment.created_at > issue.updated_at {
                issue.updated_at = comment.created_at;
            }
            issue.details_synced_at = Some(issue.updated_at);
            sql.push_str(&upsert_issue_sql(issue));
        }

        if comment.user_id.is_none() {
            if let Some(ref v) = self.viewer {
                comment.user_id = Some(v.id.clone());
                comment.user_name = v.display_name.clone();
            }
        }

        sql.push_str(&upsert_comment_sql(&comment));

        let excerpt: CompactString = comment
            .body
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>()
            .into();
        let summary: CompactString = format!("Commented: {excerpt}").into();
        let batch_id = self.next_batch_id();

        if let Some(issue) = self.issues.get(issue_id) {
            let act_ev = ActivityEvent {
                id: ActivityId::new(format!("cmt-{}", comment.id)),
                issue_id: issue.id.clone(),
                issue_identifier: issue.identifier.clone(),
                issue_title: issue.title.clone(),
                team_id: issue.team_id.clone(),
                team_key: issue.team_key.clone(),
                project_id: issue.project_id.clone(),
                project_name: issue.project_name.clone(),
                actor_id: comment.user_id.clone(),
                actor_name: comment.user_name.clone(),
                is_bot: false,
                kind: ActivityKind::Commented {
                    mentioned_viewer: false,
                    excerpt,
                },
                summary,
                body: Some(comment.body.clone()),
                url: comment.url.clone(),
                timestamp: comment.created_at,
                batch_id,
            };
            sql.push_str(&upsert_activity_sql(&act_ev));
            self.activity_events.push_front(act_ev);
        }

        let existing = self
            .comments_by_issue
            .entry(issue_id.clone())
            .or_default();
        if let Some(pos) = existing.iter().position(|c| c.id == comment.id) {
            existing[pos] = comment;
        } else {
            existing.push(comment);
        }
        existing.sort_by_key(|a| a.created_at);

        sql.push_str("COMMIT;\n");
        self.persist_sql(sql);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn apply_created_label(&mut self, label: IssueLabel) {
        self.labels.insert(label.id.clone(), label);
        self.generation = self.generation.wrapping_add(1);
    }

    fn resolve_history_names(&self, entry: &mut IssueHistoryEntry) {
        if let Some(ref from_s) = entry.from_state {
            if let Some(st) = self.states.get(&StateId::new(from_s.clone())) {
                entry.from_state = Some(st.name.clone());
            }
        }
        if let Some(ref to_s) = entry.to_state {
            if let Some(st) = self.states.get(&StateId::new(to_s.clone())) {
                entry.to_state = Some(st.name.clone());
            }
        }
        if let Some(ref from_p) = entry.from_project {
            if let Some(proj) = self.projects.get(&ProjectId::new(from_p.clone())) {
                entry.from_project = Some(proj.name.clone());
            }
        }
        if let Some(ref to_p) = entry.to_project {
            if let Some(proj) = self.projects.get(&ProjectId::new(to_p.clone())) {
                entry.to_project = Some(proj.name.clone());
            }
        }
    }

    /// Refreshes denormalized `team_key`, `team_name`, `project_name`, `state_name`, and `state_category`
    /// on all in-memory `Issue` records when parent `Team`, `Project`, or `WorkflowState` entities change.
    fn refresh_denormalized_issue_fields(&mut self, sql: &mut String) {
        for issue in self.issues.values_mut() {
            let mut changed = false;
            if let Some(team) = self.teams.get(&issue.team_id) {
                if issue.team_key != team.key || issue.team_name != team.name {
                    issue.team_key = team.key.clone();
                    issue.team_name = team.name.clone();
                    changed = true;
                }
            }
            if let Some(state) = self.states.get(&issue.state_id) {
                if issue.state_name != state.name
                    || issue.state_category != state.category
                    || issue.state_color != state.color
                {
                    issue.state_name = state.name.clone();
                    issue.state_category = state.category;
                    issue.state_color = state.color.clone();
                    changed = true;
                }
            }
            if let Some(ref pid) = issue.project_id {
                if let Some(proj) = self.projects.get(pid) {
                    if issue.project_name.as_ref() != Some(&proj.name) {
                        issue.project_name = Some(proj.name.clone());
                        changed = true;
                    }
                }
            }
            if changed {
                sql.push_str(&upsert_issue_sql(issue));
            }
        }
    }

    fn recompute_all_mention_flags(&mut self) {
        let viewer_name = self.viewer.as_ref().map(|v| v.name.clone());
        let viewer_display = self.viewer.as_ref().map(|v| v.display_name.clone());
        for (issue_id, issue) in &mut self.issues {
            let ack_at = self.sync_meta.acknowledged_mentions.get(issue_id).copied();
            if let Some(comments) = self.comments_by_issue.get(issue_id) {
                issue.has_unacknowledged_mention = comments.iter().any(|c| {
                    let is_after_ack = ack_at.map(|a| c.created_at > a).unwrap_or(true);
                    is_after_ack
                        && comment_mentions_viewer(
                            &c.body,
                            viewer_name.as_deref(),
                            viewer_display.as_deref(),
                        )
                });
            }
        }
    }

    fn next_batch_id(&mut self) -> u64 {
        let id = self.batch_counter;
        self.batch_counter = self.batch_counter.wrapping_add(1);
        id
    }

    fn persist_sql(&self, sql: String) {
        if let Some(ref handle) = self.duckdb_handle {
            handle.execute_async(sql);
        } else {
            debug!("DuckDB handle not initialized; skipping SQL batch");
        }
    }
}

pub fn comment_mentions_viewer(
    body: &str,
    viewer_name: Option<&str>,
    viewer_display: Option<&str>,
) -> bool {
    let lower = body.to_ascii_lowercase();
    for candidate in [viewer_name, viewer_display].into_iter().flatten() {
        let trimmed = candidate.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && lower.contains(&format!("@{trimmed}")) {
            return true;
        }
    }
    false
}

fn load_or_create_quack_token() -> CompactString {
    let token_path = default_quack_token_path();
    if let Ok(existing) = std::fs::read_to_string(&token_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.into();
        }
    }
    let generated: CompactString = format!("llim_{}", Utc::now().timestamp_nanos_opt().unwrap_or(42)).into();
    if let Some(parent) = token_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&token_path, generated.as_str());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&token_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&token_path, perms);
        }
    }
    generated
}

fn query_payloads<T: DeserializeOwned>(duckdb_bin: &Path, db_path: &Path, sql: &str) -> Vec<T> {
    let output = match Command::new(duckdb_bin)
        .arg("-dark-mode")
        .arg("-batch")
        .arg("-json")
        .arg(db_path)
        .arg("-c")
        .arg(sql)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    rows.into_iter()
        .filter_map(|row| {
            let payload_str = row.get("payload")?.as_str()?;
            serde_json::from_str::<T>(payload_str).ok()
        })
        .collect()
}

fn upsert_team_sql(team: &Team) -> String {
    let payload = serde_json::to_string(team).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO teams VALUES ('{}', '{}', '{}', '{}');\n",
        sql_escape(team.id.as_str()),
        sql_escape(team.key.as_str()),
        sql_escape(team.name.as_str()),
        sql_escape(&payload)
    )
}

fn upsert_state_sql(state: &WorkflowState) -> String {
    let payload = serde_json::to_string(state).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO states VALUES ('{}', '{}', '{}', '{}', '{}', '{}');\n",
        sql_escape(state.id.as_str()),
        sql_escape(state.team_id.as_str()),
        sql_escape(state.name.as_str()),
        sql_escape(state.color.as_str()),
        sql_escape(state.category.as_str()),
        sql_escape(&payload)
    )
}

fn upsert_project_sql(proj: &Project) -> String {
    let payload = serde_json::to_string(proj).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO projects VALUES ('{}', '{}', '{}', {}, {}, '{}', '{}');\n",
        sql_escape(proj.id.as_str()),
        sql_escape(proj.name.as_str()),
        sql_escape(proj.state.as_str()),
        proj.progress,
        proj.scope,
        sql_escape(&proj.updated_at.to_rfc3339()),
        sql_escape(&payload)
    )
}

fn upsert_issue_sql(issue: &Issue) -> String {
    let payload = serde_json::to_string(issue).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO issues VALUES ('{}', '{}', '{}', '{}', {}, {}, '{}', '{}', '{}', {}, '{}', {}, {}, '{}', '{}', '{}');\n",
        sql_escape(issue.id.as_str()),
        sql_escape(issue.identifier.as_str()),
        sql_escape(issue.team_key.as_str()),
        sql_escape(issue.team_name.as_str()),
        sql_opt_str(issue.project_id.as_ref().map(|p| p.as_str())),
        sql_opt_str(issue.project_name.as_deref()),
        sql_escape(issue.state_name.as_str()),
        sql_escape(issue.state_category.as_str()),
        sql_escape(issue.priority.label()),
        sql_opt_str(issue.assignee_name.as_deref()),
        sql_escape(issue.title.as_str()),
        sql_opt_str(issue.description.as_deref()),
        sql_opt_str(issue.git_branch_name.as_deref()),
        sql_escape(issue.url.as_str()),
        sql_escape(&issue.updated_at.to_rfc3339()),
        sql_escape(&payload)
    )
}

fn upsert_comment_sql(comment: &Comment) -> String {
    let payload = serde_json::to_string(comment).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO comments VALUES ('{}', '{}', '{}', '{}', '{}', {}, '{}', '{}', '{}', '{}');\n",
        sql_escape(comment.id.as_str()),
        sql_escape(comment.issue_id.as_str()),
        sql_escape(comment.issue_identifier.as_str()),
        sql_escape(comment.team_key.as_str()),
        sql_escape(comment.user_name.as_str()),
        if comment.is_bot { "TRUE" } else { "FALSE" },
        sql_escape(&comment.body),
        sql_escape(&comment.created_at.to_rfc3339()),
        sql_escape(&comment.updated_at.to_rfc3339()),
        sql_escape(&payload)
    )
}

fn upsert_history_sql(entry: &IssueHistoryEntry) -> String {
    let payload = serde_json::to_string(entry).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO issue_history VALUES ('{}', '{}', '{}', '{}', {}, {}, {}, {}, {}, '{}', '{}');\n",
        sql_escape(entry.id.as_str()),
        sql_escape(entry.issue_id.as_str()),
        sql_escape(entry.issue_identifier.as_str()),
        sql_escape(entry.team_key.as_str()),
        sql_opt_str(entry.actor_name.as_deref()),
        if entry.is_bot { "TRUE" } else { "FALSE" },
        sql_opt_str(entry.to_state.as_deref()),
        sql_opt_str(entry.to_priority.map(|p| p.label())),
        sql_opt_str(entry.to_assignee.as_deref()),
        sql_escape(&entry.created_at.to_rfc3339()),
        sql_escape(&payload)
    )
}

fn upsert_activity_sql(ev: &ActivityEvent) -> String {
    let payload = serde_json::to_string(ev).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO activity_events VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
        sql_escape(ev.id.as_str()),
        sql_escape(ev.issue_id.as_str()),
        sql_escape(ev.issue_identifier.as_str()),
        sql_escape(ev.team_key.as_str()),
        sql_escape(ev.actor_name.as_str()),
        sql_escape(ev.summary.as_str()),
        sql_escape(&ev.timestamp.to_rfc3339()),
        sql_escape(&payload)
    )
}

fn upsert_sync_meta_sql(meta: &SyncMeta) -> String {
    let payload = serde_json::to_string(meta).unwrap_or_default();
    format!(
        "INSERT OR REPLACE INTO sync_meta VALUES (1, '{}');\n",
        sql_escape(&payload)
    )
}

#[allow(dead_code)]
impl StateCategory {
    fn from_db_str(s: &str) -> Self {
        Self::from_linear_type(s)
    }
}

pub fn workflow_state_sort_key(s: &WorkflowState) -> (u8, u8) {
    let lower = s.name.as_str();
    let is_duplicate = lower.eq_ignore_ascii_case("Duplicate");
    let cat_rank = if is_duplicate {
        5
    } else {
        match s.category {
            StateCategory::Triage => 0,
            StateCategory::Backlog => 1,
            StateCategory::Unstarted => 2,
            StateCategory::Started => 3,
            StateCategory::Completed => 4,
            StateCategory::Canceled => 5,
        }
    };
    let name_rank = if lower.eq_ignore_ascii_case("Backlog")
        || lower.eq_ignore_ascii_case("Todo")
        || lower.eq_ignore_ascii_case("Planned")
        || lower.eq_ignore_ascii_case("In Progress")
        || lower.eq_ignore_ascii_case("Done")
        || lower.eq_ignore_ascii_case("Canceled")
    {
        0
    } else {
        1
    };
    (cat_rank, name_rank)
}
