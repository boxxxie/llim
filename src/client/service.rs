use std::{collections::HashSet, sync::mpsc::Sender};

use chrono::{DateTime, Utc};
use compact_str::CompactString;

use serde_json::{Map, Value};

use crate::{
    client::{
        api::LinearApi, backfill::BackfillPriorityQueue, config::ClientConfig, error::ClientError,
    },
    dispatcher::Dispatcher,
    domain::{
        Issue, IssueLabel, LinearMutation, RateLimitStatus, Team, User, WorkflowState,
    },
    event::{BootstrapPayload, DeltaBatchPayload, LlimEvent},
    id::{IssueId, ProjectId, TeamId},
};

pub struct LinearService {
    api: LinearApi,
    pub backfill_queue: BackfillPriorityQueue,
    viewer: Option<User>,
    teams: Vec<Team>,
    states: Vec<WorkflowState>,
    users: Vec<User>,
    labels: Vec<IssueLabel>,
    resolved_team_keys: Vec<CompactString>,
    unmatched_teams: Vec<CompactString>,
    issue_watermark: Option<DateTime<Utc>>,
    last_rate_limit: Option<RateLimitStatus>,
}

impl LinearService {
    pub fn new(
        config: ClientConfig,
        initial_watermark: Option<DateTime<Utc>>,
    ) -> Result<Self, ClientError> {
        let api = LinearApi::new(config)?;
        Ok(Self {
            api,
            backfill_queue: BackfillPriorityQueue::new(),
            viewer: None,
            teams: Vec::new(),
            states: Vec::new(),
            users: Vec::new(),
            labels: Vec::new(),
            resolved_team_keys: Vec::new(),
            unmatched_teams: Vec::new(),
            issue_watermark: initial_watermark,
            last_rate_limit: None,
        })
    }

    pub fn update_config(&mut self, new_config: ClientConfig) {
        let teams_changed = self.api.config().teams != new_config.teams
            || self.api.config().max_age_days != new_config.max_age_days
            || self.api.config().api_key != new_config.api_key;
        self.api.update_config(new_config);
        if teams_changed {
            self.teams.clear();
            self.states.clear();
            self.users.clear();
            self.labels.clear();
            self.resolved_team_keys.clear();
            self.issue_watermark = None;
        }
    }

    pub fn set_watermark(&mut self, watermark: Option<DateTime<Utc>>) {
        self.issue_watermark = watermark;
    }

    pub fn promote_issue(&mut self, issue_id: IssueId) {
        self.backfill_queue.promote_to_tier_zero(issue_id);
    }

    pub fn enqueue_issues(&mut self, issues: &[Issue], tier_override: Option<u8>) {
        let viewer_id = self.viewer.as_ref().map(|v| &v.id);
        for issue in issues {
            self.backfill_queue
                .enqueue_issue(issue, viewer_id, tier_override);
        }
    }

    fn update_rate_limit(&mut self, rl: Option<RateLimitStatus>, sender: &Sender<LlimEvent>) {
        if let Some(status) = rl {
            self.last_rate_limit = Some(status.clone());
            sender.dispatch(LlimEvent::RateLimitUpdated(status));
        }
    }

    fn team_ids(&self) -> Vec<TeamId> {
        self.teams.iter().map(|t| t.id.clone()).collect()
    }

    pub async fn sync(&mut self, sender: &Sender<LlimEvent>) -> Result<(), ClientError> {
        if self.api.config().offline {
            return Err(ClientError::OfflineMode);
        }

        if self.teams.is_empty() || self.viewer.is_none() {
            let tv = self.api.fetch_teams_and_viewer().await?;
            self.update_rate_limit(tv.rate_limit, sender);
            self.viewer = Some(tv.viewer);
            self.teams = tv.teams;
            self.states = tv.states;
            self.users = tv.users;
            self.labels = tv.labels;
            self.resolved_team_keys = tv.resolved_team_keys;
            self.unmatched_teams = tv.unmatched_teams.clone();

            if !tv.unmatched_teams.is_empty() {
                sender.dispatch(LlimEvent::ActivityNotice(
                    format!(
                        "Unmatched team filters ignored: {}",
                        tv.unmatched_teams.join(", ")
                    )
                    .into(),
                ));
            }
        }

        let team_ids = self.team_ids();
        if team_ids.is_empty() {
            sender.dispatch(LlimEvent::SyncError(
                "No matching Linear teams found for configured filters".into(),
            ));
            return Ok(());
        }

        match self.issue_watermark {
            None => self.run_cold_bootstrap(&team_ids, sender).await,
            Some(since) => self.run_delta_poll(&team_ids, since, sender).await,
        }
    }

    async fn run_cold_bootstrap(
        &mut self,
        team_ids: &[TeamId],
        sender: &Sender<LlimEvent>,
    ) -> Result<(), ClientError> {
        let max_pages = self.api.config().max_pages_per_poll;
        let max_age_cutoff = self
            .api
            .config()
            .max_age_days
            .map(|days| Utc::now() - chrono::Duration::days(i64::from(days)));

        let mut after: Option<String> = None;
        let mut page_count = 0u32;
        let mut all_projects = Vec::new();
        let mut all_issues = Vec::new();
        let mut first_server_time: Option<DateTime<Utc>> = None;

        loop {
            let is_first = page_count == 0;
            let page = self
                .api
                .fetch_bootstrap_page(team_ids, 50, after.as_deref(), is_first)
                .await?;
            page_count += 1;

            if first_server_time.is_none() {
                first_server_time = Some(page.server_time);
            }
            self.update_rate_limit(page.rate_limit, sender);

            if is_first {
                all_projects.extend(page.projects);
            }

            for issue in page.issues {
                if let Some(cutoff) = max_age_cutoff {
                    if !issue.is_active() && issue.updated_at < cutoff {
                        continue;
                    }
                }
                all_issues.push(issue);
            }

            if !page.has_next_page || page.end_cursor.is_none() {
                break;
            }
            if let Some(limit) = max_pages {
                if page_count >= limit {
                    break;
                }
            }
            after = page.end_cursor;
        }

        let server_time = first_server_time.unwrap_or_else(Utc::now);
        self.enqueue_issues(&all_issues, None);

        let payload = BootstrapPayload {
            viewer: self.viewer.clone(),
            resolved_team_keys: self.resolved_team_keys.clone(),
            unmatched_teams: self.unmatched_teams.clone(),
            teams: self.teams.clone(),
            states: self.states.clone(),
            users: self.users.clone(),
            labels: self.labels.clone(),
            projects: all_projects,
            issues: all_issues,
            server_time,
        };
        sender.dispatch(LlimEvent::InitialHydrationLoaded(Box::new(payload)));

        let watermark = server_time - chrono::Duration::seconds(5);
        self.issue_watermark = Some(watermark);

        // Immediately hydrate the highest-priority batch of issue comments/history
        let _ = self.step_backfill(sender).await;

        Ok(())
    }

    async fn run_delta_poll(
        &mut self,
        team_ids: &[TeamId],
        since: DateTime<Utc>,
        sender: &Sender<LlimEvent>,
    ) -> Result<(), ClientError> {
        let max_pages = self.api.config().max_pages_per_poll;
        let mut after: Option<String> = None;
        let mut page_count = 0u32;
        let mut delta_issues = Vec::new();
        let mut first_server_time: Option<DateTime<Utc>> = None;

        loop {
            let page = self
                .api
                .fetch_delta_page(team_ids, since, 50, after.as_deref())
                .await?;
            page_count += 1;

            if first_server_time.is_none() {
                first_server_time = Some(page.server_time);
            }
            self.update_rate_limit(page.rate_limit, sender);
            delta_issues.extend(page.issues);

            if !page.has_next_page || page.end_cursor.is_none() {
                break;
            }
            if let Some(limit) = max_pages {
                if page_count >= limit {
                    break;
                }
            }
            after = page.end_cursor;
        }

        let server_time = first_server_time.unwrap_or_else(Utc::now);
        let next_watermark = server_time - chrono::Duration::seconds(5);

        let mut comments_by_issue = Vec::new();
        let mut history_by_issue = Vec::new();
        let mut affected_projects: HashSet<ProjectId> = HashSet::new();

        for issue in &delta_issues {
            if let Some(ref pid) = issue.project_id {
                affected_projects.insert(pid.clone());
            }
        }

        let updated_ids: Vec<IssueId> = delta_issues.iter().map(|i| i.id.clone()).collect();
        for chunk in updated_ids.chunks(15) {
            let act_batch = self.api.fetch_issue_activities(chunk).await?;
            self.update_rate_limit(act_batch.rate_limit, sender);
            for item in act_batch.payloads {
                comments_by_issue.push((item.issue_id.clone(), item.comments));
                history_by_issue.push((item.issue_id, item.history));
            }
        }

        let refreshed_projects = if !affected_projects.is_empty() {
            let pids: Vec<ProjectId> = affected_projects.into_iter().collect();
            let (projs, rl) = self.api.fetch_projects_by_ids(&pids).await?;
            self.update_rate_limit(rl, sender);
            projs
        } else {
            Vec::new()
        };

        let payload = DeltaBatchPayload {
            resolved_team_keys: self.resolved_team_keys.clone(),
            teams: self.teams.clone(),
            states: self.states.clone(),
            users: self.users.clone(),
            labels: self.labels.clone(),
            projects: refreshed_projects,
            issues: delta_issues,
            comments_by_issue,
            history_by_issue,
            advance_issue_watermark: Some(next_watermark),
            advance_project_watermark: Some(next_watermark),
        };
        sender.dispatch(LlimEvent::DeltaBatchLoaded(Box::new(payload)));
        self.issue_watermark = Some(next_watermark);

        Ok(())
    }

    pub async fn reconcile(&mut self, sender: &Sender<LlimEvent>) -> Result<(), ClientError> {
        if self.api.config().offline {
            return Ok(());
        }
        let team_ids = self.team_ids();
        if team_ids.is_empty() {
            return Ok(());
        }
        let res = self.api.fetch_active_issue_ids(&team_ids).await?;
        self.update_rate_limit(res.rate_limit, sender);
        sender.dispatch(LlimEvent::ActiveIdsReconciled {
            active_ids: res.active_ids,
            reconciled_at: res.server_time,
        });
        Ok(())
    }

    pub async fn step_backfill(&mut self, sender: &Sender<LlimEvent>) -> Result<bool, ClientError> {
        if self.api.config().offline || self.backfill_queue.is_empty() {
            return Ok(false);
        }
        let batch = self
            .backfill_queue
            .pop_batch(15, self.last_rate_limit.as_ref());
        if batch.is_empty() {
            return Ok(false);
        }

        let res = self.api.fetch_issue_activities(&batch).await?;
        self.update_rate_limit(res.rate_limit, sender);
        for payload in res.payloads {
            sender.dispatch(LlimEvent::IssueDetailsLoaded(Box::new(payload)));
        }
        Ok(true)
    }

    pub async fn refresh_projects(
        &mut self,
        project_ids: &[ProjectId],
        sender: &Sender<LlimEvent>,
    ) -> Result<(), ClientError> {
        if self.api.config().offline || project_ids.is_empty() {
            return Ok(());
        }
        let (projects, rl) = self.api.fetch_projects_by_ids(project_ids).await?;
        self.update_rate_limit(rl, sender);
        if !projects.is_empty() {
            sender.dispatch(LlimEvent::ProjectsRefreshed(projects));
        }
        Ok(())
    }

    pub async fn execute_mutation(
        &mut self,
        mutation: LinearMutation,
        sender: &Sender<LlimEvent>,
    ) -> Result<(), ClientError> {
        self.execute_mutation_inner(mutation, sender, true).await
    }

    async fn execute_mutation_inner(
        &mut self,
        mutation: LinearMutation,
        sender: &Sender<LlimEvent>,
        show_notice: bool,
    ) -> Result<(), ClientError> {
        if self.api.config().offline {
            return Err(ClientError::OfflineMode);
        }

        match mutation {
            LinearMutation::Batch {
                mutations,
                summary_label,
            } => {
                for m in mutations {
                    Box::pin(self.execute_mutation_inner(m, sender, false)).await?;
                }
                if show_notice {
                    sender.dispatch(LlimEvent::ActivityNotice(summary_label));
                }
            }
            LinearMutation::UpdateIssue {
                issue_id,
                title,
                description,
                state_id,
                priority,
                assignee_id,
                project_id,
                label_ids,
                summary_label,
            } => {
                let mut input = Map::new();
                if let Some(t) = title {
                    input.insert("title".to_string(), Value::String(t.into()));
                }
                if let Some(d) = description {
                    input.insert("description".to_string(), Value::String(d));
                }
                if let Some(sid) = state_id {
                    input.insert("stateId".to_string(), Value::String(sid.as_str().into()));
                }
                if let Some(pri) = priority {
                    input.insert(
                        "priority".to_string(),
                        Value::Number(pri.to_linear_int().into()),
                    );
                }
                if let Some(opt_assignee) = assignee_id {
                    match opt_assignee {
                        Some(uid) => {
                            input.insert(
                                "assigneeId".to_string(),
                                Value::String(uid.as_str().into()),
                            );
                        }
                        None => {
                            input.insert("assigneeId".to_string(), Value::Null);
                        }
                    }
                }
                if let Some(opt_project) = project_id {
                    match opt_project {
                        Some(pid) => {
                            input.insert(
                                "projectId".to_string(),
                                Value::String(pid.as_str().into()),
                            );
                        }
                        None => {
                            input.insert("projectId".to_string(), Value::Null);
                        }
                    }
                }
                if let Some(lids) = label_ids {
                    input.insert(
                        "labelIds".to_string(),
                        Value::Array(
                            lids.into_iter()
                                .map(|id| Value::String(id.as_str().into()))
                                .collect(),
                        ),
                    );
                }

                let (issue, rl) = self
                    .api
                    .mutate_issue_update(&issue_id, Value::Object(input))
                    .await?;
                self.update_rate_limit(rl, sender);
                sender.dispatch(LlimEvent::IssueMutated {
                    issue: Box::new(issue),
                    summary: summary_label,
                    created: false,
                    show_notice,
                });
            }
            LinearMutation::CreateIssue {
                team_id,
                title,
                description,
                state_id,
                priority,
                assignee_id,
                project_id,
                label_ids,
            } => {
                let mut input = Map::new();
                input.insert(
                    "teamId".to_string(),
                    Value::String(team_id.as_str().to_string()),
                );
                input.insert("title".to_string(), Value::String(title.into()));
                if let Some(d) = description {
                    if !d.trim().is_empty() {
                        input.insert("description".to_string(), Value::String(d));
                    }
                }
                if let Some(sid) = state_id {
                    input.insert("stateId".to_string(), Value::String(sid.as_str().into()));
                }
                if let Some(pri) = priority {
                    input.insert(
                        "priority".to_string(),
                        Value::Number(pri.to_linear_int().into()),
                    );
                }
                if let Some(uid) = assignee_id {
                    input.insert(
                        "assigneeId".to_string(),
                        Value::String(uid.as_str().into()),
                    );
                }
                if let Some(pid) = project_id {
                    input.insert(
                        "projectId".to_string(),
                        Value::String(pid.as_str().into()),
                    );
                }
                if !label_ids.is_empty() {
                    input.insert(
                        "labelIds".to_string(),
                        Value::Array(
                            label_ids
                                .into_iter()
                                .map(|id| Value::String(id.as_str().into()))
                                .collect(),
                        ),
                    );
                }

                let (issue, rl) = self.api.mutate_issue_create(Value::Object(input)).await?;
                self.update_rate_limit(rl, sender);
                let ident = issue.identifier.clone();
                sender.dispatch(LlimEvent::IssueMutated {
                    issue: Box::new(issue),
                    summary: format!("Created issue {ident}").into(),
                    created: true,
                    show_notice,
                });
            }
            LinearMutation::CreateComment { issue_id, body } => {
                let (dto, rl) = self.api.mutate_comment_create(&issue_id, &body).await?;
                self.update_rate_limit(rl, sender);
                let comment = dto.into_domain(
                    issue_id.clone(),
                    CompactString::default(),
                    CompactString::default(),
                    "",
                );
                sender.dispatch(LlimEvent::CommentCreated {
                    issue_id,
                    comment,
                    show_notice,
                });
            }
            LinearMutation::CreateLabelAndAttach {
                issue_id,
                team_id,
                label_name,
                mut existing_label_ids,
            } => {
                let (new_label, rl1) = self
                    .api
                    .mutate_issue_label_create(&team_id, label_name.as_str())
                    .await?;
                self.update_rate_limit(rl1, sender);
                if !existing_label_ids.contains(&new_label.id) {
                    existing_label_ids.push(new_label.id.clone());
                }
                self.labels.push(new_label.clone());
                sender.dispatch(LlimEvent::LabelCreated(new_label));

                let mut input = Map::new();
                input.insert(
                    "labelIds".to_string(),
                    Value::Array(
                        existing_label_ids
                            .into_iter()
                            .map(|id| Value::String(id.as_str().into()))
                            .collect(),
                    ),
                );
                let (issue, rl2) = self
                    .api
                    .mutate_issue_update(&issue_id, Value::Object(input))
                    .await?;
                self.update_rate_limit(rl2, sender);
                sender.dispatch(LlimEvent::IssueMutated {
                    issue: Box::new(issue),
                    summary: format!("Added label +{label_name}").into(),
                    created: false,
                    show_notice,
                });
            }
        }
        Ok(())
    }
}
