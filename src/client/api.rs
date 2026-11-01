use std::time::Duration;

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use reqwest::{
    Client, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, DATE, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{
    client::{
        config::ClientConfig,
        error::ClientError,
        graphql::{
            ACTIVE_ISSUE_IDS_QUERY, ActiveIssueIdsData, BOOTSTRAP_QUERY, BootstrapData,
            COMMENT_CREATE_MUTATION, CommentCreateData, CommentNodeDto, COMPOSITE_DELTA_QUERY,
            CompositeDeltaData, GraphQlRequest, GraphQlResponse, ISSUE_ACTIVITY_BATCH_QUERY,
            ISSUE_CREATE_MUTATION, ISSUE_LABEL_CREATE_MUTATION, ISSUE_UPDATE_MUTATION,
            IssueActivityBatchData, IssueCreateData, IssueLabelCreateData, IssueUpdateData,
            PROJECT_REFRESH_QUERY, ProjectRefreshData, TEAMS_AND_VIEWER_QUERY, TeamsAndViewerData,
        },
    },
    domain::{Issue, IssueLabel, Project, RateLimitStatus, Team, User, WorkflowState},
    event::IssueActivityPayload,
    id::{IssueId, LabelId, ProjectId, TeamId},
};

#[derive(Debug, Clone)]
pub struct LinearApi {
    http: Client,
    config: ClientConfig,
}

#[derive(Debug, Clone)]
pub struct TeamsAndViewerResult {
    pub viewer: User,
    pub teams: Vec<Team>,
    pub states: Vec<WorkflowState>,
    pub users: Vec<User>,
    pub labels: Vec<IssueLabel>,
    pub resolved_team_keys: Vec<CompactString>,
    pub unmatched_teams: Vec<CompactString>,
    pub rate_limit: Option<RateLimitStatus>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BootstrapPageResult {
    pub projects: Vec<Project>,
    pub issues: Vec<Issue>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
    pub rate_limit: Option<RateLimitStatus>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DeltaPageResult {
    pub issues: Vec<Issue>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
    pub rate_limit: Option<RateLimitStatus>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActivityBatchResult {
    pub payloads: Vec<IssueActivityPayload>,
    pub rate_limit: Option<RateLimitStatus>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActiveIssueIdsResult {
    pub active_ids: Vec<(IssueId, DateTime<Utc>)>,
    pub rate_limit: Option<RateLimitStatus>,
    pub server_time: DateTime<Utc>,
}

impl LinearApi {
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { http, config })
    }

    pub fn update_config(&mut self, config: ClientConfig) {
        self.config = config;
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub async fn execute_graphql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: Value,
    ) -> Result<(T, Option<RateLimitStatus>, DateTime<Utc>), ClientError> {
        if self.config.offline {
            return Err(ClientError::OfflineMode);
        }
        let Some(ref key) = self.config.api_key else {
            return Err(ClientError::Authentication);
        };
        if key.trim().is_empty() {
            return Err(ClientError::Authentication);
        }

        let auth_header =
            HeaderValue::from_str(key.trim()).map_err(|_| ClientError::Authentication)?;

        let req_body = GraphQlRequest { query, variables };
        let resp = self
            .http
            .post(&self.config.endpoint)
            .header(AUTHORIZATION, auth_header)
            .header(CONTENT_TYPE, "application/json")
            .json(&req_body)
            .send()
            .await?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let rate_limit = parse_rate_limit_headers(&headers);
        let server_time = parse_server_date_header(&headers).unwrap_or_else(Utc::now);
        let op_name = extract_graphql_operation_name(query);

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            tracing::error!(operation = op_name, status = %status, "Linear API authentication failed");
            return Err(ClientError::Authentication);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let reset_at = rate_limit.as_ref().and_then(|r| r.reset_epoch_secs);
            tracing::warn!(operation = op_name, ?reset_at, "Linear API rate limited (HTTP 429)");
            return Err(ClientError::RateLimited { reset_at });
        }

        let body_text = resp.text().await?;
        let parsed: GraphQlResponse<T> = match serde_json::from_str(&body_text) {
            Ok(val) => val,
            Err(err) => {
                let snippet = extract_json_error_snippet(&body_text, err.line(), err.column());
                tracing::error!(
                    operation = op_name,
                    error = %err,
                    line = err.line(),
                    column = err.column(),
                    snippet = %snippet,
                    "Failed to parse Linear GraphQL response"
                );
                crate::logging::record_parse_error_dump(op_name, &err, &snippet, &body_text);
                return Err(ClientError::JsonParse(
                    format!("{op_name}: {err}").into(),
                ));
            }
        };

        if !parsed.errors.is_empty() {
            let combined = parsed
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            let lower = combined.to_lowercase();
            if lower.contains("authentication") || lower.contains("unauthorized") {
                tracing::error!(operation = op_name, errors = %combined, "Linear GraphQL auth error");
                return Err(ClientError::Authentication);
            }
            if lower.contains("ratelimit") || lower.contains("rate limit") {
                tracing::warn!(operation = op_name, errors = %combined, "Linear GraphQL rate limit error");
                return Err(ClientError::RateLimited {
                    reset_at: rate_limit.as_ref().and_then(|r| r.reset_epoch_secs),
                });
            }
            if lower.contains("complexity") {
                tracing::error!(operation = op_name, errors = %combined, "Linear GraphQL complexity exceeded");
                return Err(ClientError::ComplexityExceeded(combined.into()));
            }
            if parsed.data.is_none() {
                tracing::error!(operation = op_name, errors = %combined, "Linear GraphQL returned errors with no data");
                return Err(ClientError::GraphQlPartialError(combined.into()));
            }
            tracing::warn!(operation = op_name, "Linear GraphQL returned partial errors: {}", combined);
        }

        let Some(data) = parsed.data else {
            tracing::error!(operation = op_name, "Linear GraphQL response contained neither data nor errors");
            return Err(ClientError::GraphQlPartialError(
                "Linear GraphQL response contained neither data nor errors".into(),
            ));
        };

        Ok((data, rate_limit, server_time))
    }

    pub async fn fetch_teams_and_viewer(&self) -> Result<TeamsAndViewerResult, ClientError> {
        let (data, rate_limit, server_time): (
            TeamsAndViewerData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self
            .execute_graphql(TEAMS_AND_VIEWER_QUERY, json!({}))
            .await?;

        let viewer = data.viewer.into_domain();
        let all_tuples: Vec<TeamBundle> = data
            .teams
            .nodes
            .into_iter()
            .map(|t| t.into_domain())
            .collect();

        let filtered = filter_configured_teams(all_tuples, &self.config.teams);

        Ok(TeamsAndViewerResult {
            viewer,
            teams: filtered.teams,
            states: filtered.states,
            users: filtered.users,
            labels: filtered.labels,
            resolved_team_keys: filtered.resolved_team_keys,
            unmatched_teams: filtered.unmatched_teams,
            rate_limit,
            server_time,
        })
    }

    pub async fn fetch_bootstrap_page(
        &self,
        team_ids: &[TeamId],
        first: usize,
        after: Option<&str>,
        include_projects: bool,
    ) -> Result<BootstrapPageResult, ClientError> {
        let team_id_strs: Vec<&str> = team_ids.iter().map(|id| id.as_str()).collect();
        let vars = json!({
            "teamIds": team_id_strs,
            "first": first,
            "after": after,
            "includeProjects": include_projects,
        });

        let (data, rate_limit, server_time): (
            BootstrapData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(BOOTSTRAP_QUERY, vars).await?;

        let projects = data
            .projects
            .map(|c| c.nodes.into_iter().map(|p| p.into_domain()).collect())
            .unwrap_or_default();
        let has_next_page = data
            .issues
            .page_info
            .as_ref()
            .map(|pi| pi.has_next_page)
            .unwrap_or(false);
        let end_cursor = data.issues.page_info.and_then(|pi| pi.end_cursor);
        let issues = data
            .issues
            .nodes
            .into_iter()
            .map(|i| i.into_domain())
            .collect();

        Ok(BootstrapPageResult {
            projects,
            issues,
            has_next_page,
            end_cursor,
            rate_limit,
            server_time,
        })
    }

    pub async fn fetch_delta_page(
        &self,
        team_ids: &[TeamId],
        since: DateTime<Utc>,
        first: usize,
        after: Option<&str>,
    ) -> Result<DeltaPageResult, ClientError> {
        let team_id_strs: Vec<&str> = team_ids.iter().map(|id| id.as_str()).collect();
        let vars = json!({
            "teamIds": team_id_strs,
            "since": since.to_rfc3339(),
            "first": first,
            "after": after,
        });

        let (data, rate_limit, server_time): (
            CompositeDeltaData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(COMPOSITE_DELTA_QUERY, vars).await?;

        let has_next_page = data
            .issues
            .page_info
            .as_ref()
            .map(|pi| pi.has_next_page)
            .unwrap_or(false);
        let end_cursor = data.issues.page_info.and_then(|pi| pi.end_cursor);
        let issues = data
            .issues
            .nodes
            .into_iter()
            .map(|i| i.into_domain())
            .collect();

        Ok(DeltaPageResult {
            issues,
            has_next_page,
            end_cursor,
            rate_limit,
            server_time,
        })
    }

    pub async fn fetch_issue_activities(
        &self,
        issue_ids: &[IssueId],
    ) -> Result<ActivityBatchResult, ClientError> {
        if issue_ids.is_empty() {
            return Ok(ActivityBatchResult {
                payloads: Vec::new(),
                rate_limit: None,
                server_time: Utc::now(),
            });
        }

        let ids: Vec<&str> = issue_ids.iter().map(|id| id.as_str()).collect();
        let vars = json!({ "issueIds": ids });

        let (data, rate_limit, server_time): (
            IssueActivityBatchData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self
            .execute_graphql(ISSUE_ACTIVITY_BATCH_QUERY, vars)
            .await?;

        let payloads = data
            .issues
            .nodes
            .into_iter()
            .map(|n| n.into_payload())
            .collect();

        Ok(ActivityBatchResult {
            payloads,
            rate_limit,
            server_time,
        })
    }

    pub async fn fetch_active_issue_ids(
        &self,
        team_ids: &[TeamId],
    ) -> Result<ActiveIssueIdsResult, ClientError> {
        let team_id_strs: Vec<&str> = team_ids.iter().map(|id| id.as_str()).collect();
        let mut active_ids = Vec::new();
        let mut after: Option<String> = None;
        let mut last_rate_limit = None;
        let mut last_server_time;

        loop {
            let vars = json!({
                "teamIds": team_id_strs,
                "after": after,
            });
            let (data, rate_limit, server_time): (
                ActiveIssueIdsData,
                Option<RateLimitStatus>,
                DateTime<Utc>,
            ) = self.execute_graphql(ACTIVE_ISSUE_IDS_QUERY, vars).await?;

            if rate_limit.is_some() {
                last_rate_limit = rate_limit;
            }
            last_server_time = server_time;

            for node in data.issues.nodes {
                active_ids.push((IssueId::new(node.id), node.updated_at));
            }

            let has_next = data
                .issues
                .page_info
                .as_ref()
                .map(|p| p.has_next_page)
                .unwrap_or(false);
            after = data.issues.page_info.and_then(|p| p.end_cursor);
            if !has_next || after.is_none() {
                break;
            }
        }

        Ok(ActiveIssueIdsResult {
            active_ids,
            rate_limit: last_rate_limit,
            server_time: last_server_time,
        })
    }

    pub async fn fetch_projects_by_ids(
        &self,
        project_ids: &[ProjectId],
    ) -> Result<(Vec<Project>, Option<RateLimitStatus>), ClientError> {
        if project_ids.is_empty() {
            return Ok((Vec::new(), None));
        }
        let ids: Vec<&str> = project_ids.iter().map(|id| id.as_str()).collect();
        let vars = json!({ "projectIds": ids });
        let (data, rate_limit, _): (
            ProjectRefreshData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(PROJECT_REFRESH_QUERY, vars).await?;

        let projects = data
            .projects
            .nodes
            .into_iter()
            .map(|p| p.into_domain())
            .collect();
        Ok((projects, rate_limit))
    }

    pub async fn mutate_issue_update(
        &self,
        issue_id: &IssueId,
        input: Value,
    ) -> Result<(Issue, Option<RateLimitStatus>), ClientError> {
        let vars = json!({
            "id": issue_id.as_str(),
            "input": input,
        });
        let (data, rate_limit, _): (
            IssueUpdateData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(ISSUE_UPDATE_MUTATION, vars).await?;

        let Some(issue_node) = data.issue_update.issue else {
            return Err(ClientError::GraphQlPartialError(
                "issueUpdate returned no issue node".into(),
            ));
        };
        Ok((issue_node.into_domain(), rate_limit))
    }

    pub async fn mutate_issue_create(
        &self,
        input: Value,
    ) -> Result<(Issue, Option<RateLimitStatus>), ClientError> {
        let vars = json!({ "input": input });
        let (data, rate_limit, _): (
            IssueCreateData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(ISSUE_CREATE_MUTATION, vars).await?;

        let Some(issue_node) = data.issue_create.issue else {
            return Err(ClientError::GraphQlPartialError(
                "issueCreate returned no issue node".into(),
            ));
        };
        Ok((issue_node.into_domain(), rate_limit))
    }

    pub async fn mutate_comment_create(
        &self,
        issue_id: &IssueId,
        body: &str,
    ) -> Result<(CommentNodeDto, Option<RateLimitStatus>), ClientError> {
        let vars = json!({
            "input": {
                "issueId": issue_id.as_str(),
                "body": body,
            }
        });
        let (data, rate_limit, _): (
            CommentCreateData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self.execute_graphql(COMMENT_CREATE_MUTATION, vars).await?;

        let Some(comment_node) = data.comment_create.comment else {
            return Err(ClientError::GraphQlPartialError(
                "commentCreate returned no comment node".into(),
            ));
        };
        Ok((comment_node, rate_limit))
    }

    pub async fn mutate_issue_label_create(
        &self,
        team_id: &TeamId,
        name: &str,
    ) -> Result<(IssueLabel, Option<RateLimitStatus>), ClientError> {
        let vars = json!({
            "input": {
                "teamId": team_id.as_str(),
                "name": name,
            }
        });
        let (data, rate_limit, _): (
            IssueLabelCreateData,
            Option<RateLimitStatus>,
            DateTime<Utc>,
        ) = self
            .execute_graphql(ISSUE_LABEL_CREATE_MUTATION, vars)
            .await?;

        let Some(label_node) = data.issue_label_create.issue_label else {
            return Err(ClientError::GraphQlPartialError(
                "issueLabelCreate returned no label node".into(),
            ));
        };
        let label = IssueLabel {
            id: LabelId::new(label_node.id.unwrap_or_else(|| name.to_string())),
            team_id: Some(team_id.clone()),
            name: label_node.name.into(),
        };
        Ok((label, rate_limit))
    }
}

pub type TeamBundle = (Team, Vec<WorkflowState>, Vec<User>, Vec<IssueLabel>);

pub struct FilteredTeamsResult {
    pub teams: Vec<Team>,
    pub states: Vec<WorkflowState>,
    pub users: Vec<User>,
    pub labels: Vec<IssueLabel>,
    pub resolved_team_keys: Vec<CompactString>,
    pub unmatched_teams: Vec<CompactString>,
}

pub fn filter_configured_teams(
    all_tuples: Vec<TeamBundle>,
    configured: &[CompactString],
) -> FilteredTeamsResult {
    let cleaned: Vec<CompactString> = configured
        .iter()
        .map(|s| CompactString::from(s.trim()))
        .filter(|s| !s.is_empty())
        .collect();

    if cleaned.is_empty() {
        let mut teams = Vec::new();
        let mut states = Vec::new();
        let mut users = Vec::new();
        let mut labels = Vec::new();
        let mut keys = Vec::new();
        for (t, st, us, lb) in all_tuples {
            keys.push(t.key.clone());
            teams.push(t);
            states.extend(st);
            users.extend(us);
            labels.extend(lb);
        }
        return FilteredTeamsResult {
            teams,
            states,
            users,
            labels,
            resolved_team_keys: keys,
            unmatched_teams: Vec::new(),
        };
    }

    let mut matched_teams = Vec::new();
    let mut matched_states = Vec::new();
    let mut matched_users = Vec::new();
    let mut matched_labels = Vec::new();
    let mut resolved_keys = Vec::new();
    let mut unmatched = Vec::new();

    for req in &cleaned {
        let found = all_tuples.iter().any(|(t, _, _, _)| {
            t.key.eq_ignore_ascii_case(req.as_str()) || t.name.eq_ignore_ascii_case(req.as_str())
        });
        if !found {
            unmatched.push(req.clone());
        }
    }

    for (t, st, us, lb) in all_tuples {
        let is_match = cleaned.iter().any(|req| {
            t.key.eq_ignore_ascii_case(req.as_str()) || t.name.eq_ignore_ascii_case(req.as_str())
        });
        if is_match {
            resolved_keys.push(t.key.clone());
            matched_teams.push(t);
            matched_states.extend(st);
            matched_users.extend(us);
            matched_labels.extend(lb);
        }
    }

    FilteredTeamsResult {
        teams: matched_teams,
        states: matched_states,
        users: matched_users,
        labels: matched_labels,
        resolved_team_keys: resolved_keys,
        unmatched_teams: unmatched,
    }
}

fn parse_rate_limit_headers(headers: &HeaderMap) -> Option<RateLimitStatus> {
    let get_u32 = |name: &str| -> Option<u32> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
    };
    let get_i64 = |name: &str| -> Option<i64> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok())
    };

    let requests_remaining = get_u32("x-ratelimit-requests-remaining");
    let requests_limit = get_u32("x-ratelimit-requests-limit");
    let complexity_remaining = get_u32("x-ratelimit-complexity-remaining");
    let complexity_limit = get_u32("x-ratelimit-complexity-limit");
    let reset_epoch_secs = get_i64("x-ratelimit-requests-reset")
        .or_else(|| get_i64("x-ratelimit-complexity-reset"));

    if requests_remaining.is_none()
        && requests_limit.is_none()
        && complexity_remaining.is_none()
        && complexity_limit.is_none()
    {
        return None;
    }

    Some(RateLimitStatus {
        requests_remaining,
        requests_limit,
        complexity_remaining,
        complexity_limit,
        reset_epoch_secs,
    })
}

fn parse_server_date_header(headers: &HeaderMap) -> Option<DateTime<Utc>> {
    let raw = headers.get(DATE)?.to_str().ok()?;
    DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn extract_graphql_operation_name(query: &str) -> &str {
    for line in query.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("query ")
            .or_else(|| trimmed.strip_prefix("mutation "))
        {
            let name = rest
                .split(['(', '{', ' '])
                .next()
                .unwrap_or("GraphQL")
                .trim();
            if !name.is_empty() {
                return name;
            }
        }
    }
    "GraphQL"
}

fn extract_json_error_snippet(body: &str, line: usize, column: usize) -> String {
    let target_line = body
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or_else(|| body.lines().next().unwrap_or(""));
    let char_idx = column.saturating_sub(1);
    let chars: Vec<char> = target_line.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let start = char_idx.saturating_sub(120).min(chars.len());
    let end = (char_idx + 120).min(chars.len());
    chars[start..end].iter().collect()
}
