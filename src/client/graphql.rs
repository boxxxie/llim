use chrono::{DateTime, Utc};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{
        Comment, Issue, IssueHistoryEntry, IssueLabel, IssuePriority, Project,
        RelatedIssueSummary, StateCategory, SubIssueSummary, Team, User, WorkflowState,
    },
    event::IssueActivityPayload,
    id::{CommentId, HistoryId, IssueId, LabelId, ProjectId, StateId, TeamId, UserId},
};

pub const TEAMS_AND_VIEWER_QUERY: &str = r#"
query TeamsAndViewer {
  viewer {
    id
    name
    displayName
    email
  }
  teams(first: 15) {
    nodes {
      id
      key
      name
      states(first: 50) {
        nodes {
          id
          name
          type
          color
        }
      }
      members(first: 25) {
        nodes {
          id
          name
          displayName
          email
        }
      }
      labels(first: 50) {
        nodes {
          id
          name
        }
      }
    }
  }
}
"#;

pub const BOOTSTRAP_QUERY: &str = r#"
query BootstrapIssues(
  $teamIds: [ID!]!
  $first: Int!
  $after: String
  $includeProjects: Boolean!
) {
  projects(
    first: 50
    filter: { accessibleTeams: { id: { in: $teamIds } } }
  ) @include(if: $includeProjects) {
    nodes {
      id
      name
      description
      state
      progress
      scope
      targetDate
      updatedAt
      archivedAt
      url
      teams(first: 5) { nodes { id key } }
    }
  }
  issues(
    first: $first
    after: $after
    orderBy: updatedAt
    filter: {
      team: { id: { in: $teamIds } }
    }
  ) {
    pageInfo {
      hasNextPage
      endCursor
    }
    nodes {
      id
      identifier
      title
      description
      url
      branchName
      priority
      createdAt
      updatedAt
      completedAt
      canceledAt
      archivedAt
      team { id key name }
      project { id name }
      state { id name type color }
      assignee { id name displayName }
      creator { id name displayName }
      parent { id identifier title }
      children(first: 25) {
        nodes {
          id
          identifier
          title
          priority
          state { name type color }
          assignee { id name displayName }
        }
      }
      labels(first: 10) { nodes { id name } }
      relations(first: 5) {
        nodes {
          id
          type
          relatedIssue { id identifier title state { name type color } }
        }
      }
      inverseRelations(first: 5) {
        nodes {
          id
          type
          issue { id identifier title state { name type color } }
        }
      }
    }
  }
}
"#;

pub const COMPOSITE_DELTA_QUERY: &str = r#"
query CompositeDelta(
  $teamIds: [ID!]!
  $since: DateTimeOrDuration!
  $first: Int!
  $after: String
) {
  issues(
    first: $first
    after: $after
    orderBy: updatedAt
    filter: {
      team: { id: { in: $teamIds } }
      updatedAt: { gt: $since }
    }
  ) {
    pageInfo {
      hasNextPage
      endCursor
    }
    nodes {
      id
      identifier
      title
      description
      url
      branchName
      priority
      createdAt
      updatedAt
      completedAt
      canceledAt
      archivedAt
      team { id key name }
      project { id name }
      state { id name type color }
      assignee { id name displayName }
      creator { id name displayName }
      parent { id identifier title }
      children(first: 25) {
        nodes {
          id
          identifier
          title
          priority
          state { name type color }
          assignee { id name displayName }
        }
      }
      labels(first: 10) { nodes { id name } }
      relations(first: 5) {
        nodes {
          id
          type
          relatedIssue { id identifier title state { name type color } }
        }
      }
      inverseRelations(first: 5) {
        nodes {
          id
          type
          issue { id identifier title state { name type color } }
        }
      }
    }
  }
}
"#;

pub const ISSUE_ACTIVITY_BATCH_QUERY: &str = r#"
query IssueActivityBatch($issueIds: [ID!]!) {
  issues(first: 15, filter: { id: { in: $issueIds } }) {
    nodes {
      id
      identifier
      updatedAt
      url
      team { id key name }
      comments(first: 15) {
        nodes {
          id
          body
          createdAt
          updatedAt
          url
          user { id name displayName }
          botActor { name }
        }
      }
      history(first: 20) {
        nodes {
          id
          createdAt
          updatedDescription
          fromTitle
          toTitle
          actor { id name displayName }
          botActor { name }
          fromState { id name }
          toState { id name }
          fromAssignee { id name displayName }
          toAssignee { id name displayName }
          fromPriority
          toPriority
          fromProject { id name }
          toProject { id name }
          fromParent { id identifier title }
          toParent { id identifier title }
          addedLabels { id name }
          removedLabels { id name }
        }
      }
    }
  }
}
"#;

pub const ACTIVE_ISSUE_IDS_QUERY: &str = r#"
query ActiveIssueIds($teamIds: [ID!]!, $after: String) {
  issues(
    first: 250
    after: $after
    filter: {
      team: { id: { in: $teamIds } }
      state: { type: { nin: ["completed", "canceled"] } }
    }
  ) {
    pageInfo {
      hasNextPage
      endCursor
    }
    nodes {
      id
      updatedAt
    }
  }
}
"#;

pub const PROJECT_REFRESH_QUERY: &str = r#"
query ProjectRefresh($projectIds: [ID!]!) {
  projects(first: 50, filter: { id: { in: $projectIds } }) {
    nodes {
      id
      name
      description
      state
      progress
      scope
      targetDate
      updatedAt
      archivedAt
      url
      teams(first: 5) { nodes { id key } }
    }
  }
}
"#;

pub const ISSUE_UPDATE_MUTATION: &str = r#"
mutation IssueUpdate($id: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $id, input: $input) {
    success
    issue {
      id
      identifier
      title
      description
      url
      branchName
      priority
      createdAt
      updatedAt
      completedAt
      canceledAt
      archivedAt
      team { id key name }
      project { id name }
      state { id name type color }
      assignee { id name displayName }
      creator { id name displayName }
      parent { id identifier title }
      children(first: 25) {
        nodes {
          id
          identifier
          title
          priority
          state { name type color }
          assignee { id name displayName }
        }
      }
      labels(first: 10) { nodes { id name } }
      relations(first: 5) {
        nodes {
          id
          type
          relatedIssue { id identifier title state { name type color } }
        }
      }
      inverseRelations(first: 5) {
        nodes {
          id
          type
          issue { id identifier title state { name type color } }
        }
      }
    }
  }
}
"#;

pub const ISSUE_CREATE_MUTATION: &str = r#"
mutation IssueCreate($input: IssueCreateInput!) {
  issueCreate(input: $input) {
    success
    issue {
      id
      identifier
      title
      description
      url
      branchName
      priority
      createdAt
      updatedAt
      completedAt
      canceledAt
      archivedAt
      team { id key name }
      project { id name }
      state { id name type color }
      assignee { id name displayName }
      creator { id name displayName }
      parent { id identifier title }
      children(first: 25) {
        nodes {
          id
          identifier
          title
          priority
          state { name type color }
          assignee { id name displayName }
        }
      }
      labels(first: 10) { nodes { id name } }
      relations(first: 5) {
        nodes {
          id
          type
          relatedIssue { id identifier title state { name type color } }
        }
      }
      inverseRelations(first: 5) {
        nodes {
          id
          type
          issue { id identifier title state { name type color } }
        }
      }
    }
  }
}
"#;

pub const COMMENT_CREATE_MUTATION: &str = r#"
mutation CommentCreate($input: CommentCreateInput!) {
  commentCreate(input: $input) {
    success
    comment {
      id
      body
      createdAt
      updatedAt
      url
      user { id name displayName }
      botActor { name }
    }
  }
}
"#;

pub const ISSUE_LABEL_CREATE_MUTATION: &str = r#"
mutation IssueLabelCreate($input: IssueLabelCreateInput!) {
  issueLabelCreate(input: $input) {
    success
    issueLabel {
      id
      name
    }
  }
}
"#;

#[derive(Debug, Clone, Serialize)]
pub struct GraphQlRequest<'a> {
    pub query: &'a str,
    pub variables: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQlResponse<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GraphQlErrorItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQlErrorItem {
    pub message: String,
    #[serde(default)]
    pub extensions: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Connection<T> {
    #[serde(default)]
    pub nodes: Vec<T>,
    #[serde(rename = "pageInfo")]
    pub page_info: Option<PageInfoDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageInfoDto {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamsAndViewerData {
    pub viewer: ViewerDto,
    pub teams: Connection<TeamNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ViewerDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub email: Option<String>,
}

impl ViewerDto {
    pub fn into_domain(self) -> User {
        let display = self
            .display_name
            .unwrap_or_else(|| self.name.clone())
            .into();
        User {
            id: UserId::new(self.id),
            name: self.name.into(),
            display_name: display,
            email: self.email.map(Into::into),
            is_bot: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamNodeDto {
    pub id: String,
    pub key: String,
    pub name: String,
    pub states: Option<Connection<StateNodeDto>>,
    #[serde(default)]
    pub members: Option<Connection<ViewerDto>>,
    #[serde(default)]
    pub labels: Option<Connection<LabelNodeDto>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateNodeDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub state_type: String,
    pub color: Option<String>,
}

impl TeamNodeDto {
    pub fn into_domain(self) -> (Team, Vec<WorkflowState>, Vec<User>, Vec<IssueLabel>) {
        let team_id = TeamId::new(&self.id);
        let states = self
            .states
            .map(|c| {
                c.nodes
                    .into_iter()
                    .map(|s| WorkflowState {
                        id: StateId::new(s.id),
                        team_id: team_id.clone(),
                        name: s.name.into(),
                        color: s.color.unwrap_or_else(|| "#808080".to_string()).into(),
                        category: StateCategory::from_linear_type(&s.state_type),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let members = self
            .members
            .map(|c| c.nodes.into_iter().map(|m| m.into_domain()).collect())
            .unwrap_or_default();
        let labels = self
            .labels
            .map(|c| {
                c.nodes
                    .into_iter()
                    .filter_map(|l| {
                        l.id.map(|id| IssueLabel {
                            id: LabelId::new(id),
                            team_id: Some(team_id.clone()),
                            name: l.name.into(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let team = Team {
            id: team_id,
            key: self.key.into(),
            name: self.name.into(),
        };
        (team, states, members, labels)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapData {
    pub projects: Option<Connection<ProjectNodeDto>>,
    pub issues: Connection<IssueNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompositeDeltaData {
    pub issues: Connection<IssueNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueActivityBatchData {
    pub issues: Connection<IssueActivityNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActiveIssueIdsData {
    pub issues: Connection<ActiveIssueIdNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActiveIssueIdNodeDto {
    pub id: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectRefreshData {
    pub projects: Connection<ProjectNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamKeyRefDto {
    pub id: String,
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectNodeDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub state: Option<String>,
    pub progress: Option<f64>,
    pub scope: Option<f64>,
    #[serde(rename = "targetDate")]
    pub target_date: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "archivedAt")]
    pub archived_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
    pub teams: Option<Connection<TeamKeyRefDto>>,
}

impl ProjectNodeDto {
    pub fn into_domain(self) -> Project {
        let mut team_ids = Vec::new();
        let mut team_keys = Vec::new();
        if let Some(conn) = self.teams {
            for t in conn.nodes {
                team_ids.push(TeamId::new(t.id));
                if let Some(k) = t.key {
                    team_keys.push(k.into());
                }
            }
        }
        Project {
            id: ProjectId::new(self.id),
            name: self.name.into(),
            description: self.description.unwrap_or_default().into(),
            state: self.state.unwrap_or_else(|| "started".to_string()).into(),
            progress: self.progress.unwrap_or(0.0),
            scope: self.scope.unwrap_or(0.0),
            target_date: self.target_date.map(Into::into),
            url: self.url.unwrap_or_default().into(),
            team_ids,
            team_keys,
            updated_at: self.updated_at,
            archived_at: self.archived_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamRefDto {
    pub id: String,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectRefDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserNodeDto {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotActorDto {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelNodeDto {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelatedIssueStateDto {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub state_type: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelatedIssueRefDto {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub title: Option<String>,
    pub state: Option<RelatedIssueStateDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationNodeDto {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub rel_type: String,
    #[serde(rename = "relatedIssue")]
    pub related_issue: Option<RelatedIssueRefDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InverseRelationNodeDto {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub rel_type: String,
    pub issue: Option<RelatedIssueRefDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParentIssueRefDto {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChildIssueNodeDto {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub title: String,
    pub priority: Option<i32>,
    pub state: Option<RelatedIssueStateDto>,
    pub assignee: Option<UserNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueNodeDto {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    #[serde(rename = "branchName")]
    pub branch_name: Option<String>,
    pub priority: Option<i32>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(rename = "canceledAt")]
    pub canceled_at: Option<DateTime<Utc>>,
    #[serde(rename = "archivedAt")]
    pub archived_at: Option<DateTime<Utc>>,
    pub team: TeamRefDto,
    pub project: Option<ProjectRefDto>,
    pub state: StateNodeDto,
    pub assignee: Option<UserNodeDto>,
    pub creator: Option<UserNodeDto>,
    #[serde(default)]
    pub parent: Option<ParentIssueRefDto>,
    #[serde(default)]
    pub children: Option<Connection<ChildIssueNodeDto>>,
    pub labels: Option<Connection<LabelNodeDto>>,
    #[serde(default)]
    pub relations: Option<Connection<RelationNodeDto>>,
    #[serde(rename = "inverseRelations")]
    pub inverse_relations: Option<Connection<InverseRelationNodeDto>>,
}

impl IssueNodeDto {
    pub fn into_domain(self) -> Issue {
        let mut blocked_by = Vec::new();
        let mut related_issues = Vec::new();

        if let Some(rel_conn) = self.relations {
            for r in rel_conn.nodes {
                if let Some(target) = r.related_issue {
                    let (state_name, state_color, state_category) = match target.state {
                        Some(s) => (
                            s.name.unwrap_or_else(|| s.state_type.clone()).into(),
                            s.color.unwrap_or_else(|| "#808080".to_string()).into(),
                            StateCategory::from_linear_type(&s.state_type),
                        ),
                        None => ("Unknown".into(), "#808080".into(), StateCategory::Unstarted),
                    };
                    related_issues.push(RelatedIssueSummary {
                        id: IssueId::new(target.id),
                        identifier: target.identifier.into(),
                        title: target.title.unwrap_or_default().into(),
                        relation_type: r.rel_type.into(),
                        state_name,
                        state_color,
                        state_category,
                    });
                }
            }
        }

        if let Some(inv_conn) = self.inverse_relations {
            for r in inv_conn.nodes {
                if let Some(source) = r.issue {
                    let (state_name, state_color, cat) = match source.state {
                        Some(s) => (
                            s.name.unwrap_or_else(|| s.state_type.clone()).into(),
                            s.color.unwrap_or_else(|| "#808080".to_string()).into(),
                            StateCategory::from_linear_type(&s.state_type),
                        ),
                        None => ("Unknown".into(), "#808080".into(), StateCategory::Unstarted),
                    };
                    if r.rel_type == "blocks" && cat.is_active() {
                        blocked_by.push(source.identifier.clone().into());
                    }
                    let inv_label = if r.rel_type == "blocks" {
                        "blocked_by"
                    } else {
                        r.rel_type.as_str()
                    };
                    if !related_issues
                        .iter()
                        .any(|existing| existing.identifier.as_str() == source.identifier.as_str())
                    {
                        related_issues.push(RelatedIssueSummary {
                            id: IssueId::new(source.id),
                            identifier: source.identifier.into(),
                            title: source.title.unwrap_or_default().into(),
                            relation_type: inv_label.into(),
                            state_name,
                            state_color,
                            state_category: cat,
                        });
                    }
                }
            }
        }

        let (parent_id, parent_identifier, parent_title) = match self.parent {
            Some(p) => (
                Some(IssueId::new(p.id)),
                Some(p.identifier.into()),
                p.title.map(Into::into),
            ),
            None => (None, None, None),
        };

        let mut sub_issues = Vec::new();
        if let Some(child_conn) = self.children {
            for c in child_conn.nodes {
                let (state_name, state_color, state_category) = match c.state {
                    Some(s) => (
                        s.name.unwrap_or_else(|| s.state_type.clone()).into(),
                        s.color.unwrap_or_else(|| "#808080".to_string()).into(),
                        StateCategory::from_linear_type(&s.state_type),
                    ),
                    None => ("Todo".into(), "#808080".into(), StateCategory::Unstarted),
                };
                let assignee_name = c
                    .assignee
                    .map(|a| a.display_name.unwrap_or(a.name).into());
                sub_issues.push(SubIssueSummary {
                    id: IssueId::new(c.id),
                    identifier: c.identifier.into(),
                    title: c.title.into(),
                    state_name,
                    state_color,
                    state_category,
                    priority: IssuePriority::from_linear_int(c.priority.unwrap_or(0)),
                    assignee_name,
                });
            }
        }
        sub_issues.sort_by(|a, b| {
            a.state_category
                .cmp(&b.state_category)
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| a.identifier.cmp(&b.identifier))
        });

        let mut labels = Vec::new();
        let mut label_ids = Vec::new();
        if let Some(lbl_conn) = self.labels {
            for l in lbl_conn.nodes {
                labels.push(l.name.into());
                if let Some(id) = l.id {
                    label_ids.push(LabelId::new(id));
                }
            }
        }

        let (assignee_id, assignee_name) = match self.assignee {
            Some(a) => (
                Some(UserId::new(a.id)),
                Some(a.display_name.unwrap_or(a.name).into()),
            ),
            None => (None, None),
        };

        let (creator_id, creator_name) = match self.creator {
            Some(c) => (
                Some(UserId::new(c.id)),
                Some(c.display_name.unwrap_or(c.name).into()),
            ),
            None => (None, None),
        };

        let (project_id, project_name) = match self.project {
            Some(p) => (Some(ProjectId::new(p.id)), Some(p.name.into())),
            None => (None, None),
        };

        Issue {
            id: IssueId::new(self.id),
            identifier: self.identifier.into(),
            title: self.title.into(),
            description: self.description,
            team_id: TeamId::new(self.team.id),
            team_key: self.team.key.into(),
            team_name: self.team.name.into(),
            project_id,
            project_name,
            state_id: StateId::new(self.state.id),
            state_name: self.state.name.into(),
            state_color: self
                .state
                .color
                .unwrap_or_else(|| "#808080".to_string())
                .into(),
            state_category: StateCategory::from_linear_type(&self.state.state_type),
            priority: IssuePriority::from_linear_int(self.priority.unwrap_or(0)),
            assignee_id,
            assignee_name,
            creator_id,
            creator_name,
            labels,
            label_ids,
            blocked_by,
            parent_id,
            parent_identifier,
            parent_title,
            sub_issues,
            related_issues,
            has_unacknowledged_mention: false,
            git_branch_name: self.branch_name.map(Into::into),
            url: self.url.into(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            completed_at: self.completed_at,
            canceled_at: self.canceled_at,
            archived_at: self.archived_at,
            details_synced_at: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueActivityNodeDto {
    pub id: String,
    pub identifier: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    pub url: String,
    pub team: TeamRefDto,
    pub comments: Option<Connection<CommentNodeDto>>,
    pub history: Option<Connection<HistoryNodeDto>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentNodeDto {
    pub id: String,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
    pub user: Option<UserNodeDto>,
    #[serde(rename = "botActor")]
    pub bot_actor: Option<BotActorDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryStateRefDto {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryProjectRefDto {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryNodeDto {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedDescription", default)]
    pub updated_description: Option<bool>,
    #[serde(rename = "fromTitle", default)]
    pub from_title: Option<String>,
    #[serde(rename = "toTitle", default)]
    pub to_title: Option<String>,
    pub actor: Option<UserNodeDto>,
    #[serde(rename = "botActor")]
    pub bot_actor: Option<BotActorDto>,
    #[serde(rename = "fromState")]
    pub from_state: Option<HistoryStateRefDto>,
    #[serde(rename = "toState")]
    pub to_state: Option<HistoryStateRefDto>,
    #[serde(rename = "fromAssignee")]
    pub from_assignee: Option<UserNodeDto>,
    #[serde(rename = "toAssignee")]
    pub to_assignee: Option<UserNodeDto>,
    #[serde(rename = "fromPriority")]
    pub from_priority: Option<i32>,
    #[serde(rename = "toPriority")]
    pub to_priority: Option<i32>,
    #[serde(rename = "fromProject")]
    pub from_project: Option<HistoryProjectRefDto>,
    #[serde(rename = "toProject")]
    pub to_project: Option<HistoryProjectRefDto>,
    #[serde(rename = "fromParent", default)]
    pub from_parent: Option<ParentIssueRefDto>,
    #[serde(rename = "toParent", default)]
    pub to_parent: Option<ParentIssueRefDto>,
    #[serde(rename = "addedLabels", default)]
    pub added_labels: Option<Vec<LabelNodeDto>>,
    #[serde(rename = "removedLabels", default)]
    pub removed_labels: Option<Vec<LabelNodeDto>>,
}

impl IssueActivityNodeDto {
    pub fn into_payload(self) -> IssueActivityPayload {
        let issue_id = IssueId::new(self.id);
        let issue_identifier: CompactString = self.identifier.into();
        let team_key: CompactString = self.team.key.into();
        let synced_updated_at = self.updated_at.unwrap_or_else(Utc::now);

        let mut comments = Vec::new();
        if let Some(comment_conn) = self.comments {
            for c in comment_conn.nodes {
                let (user_id, user_name, is_bot) = resolve_actor(c.user, c.bot_actor);
                let url = c.url.unwrap_or_else(|| self.url.clone()).into();
                comments.push(Comment {
                    id: CommentId::new(c.id),
                    issue_id: issue_id.clone(),
                    issue_identifier: issue_identifier.clone(),
                    team_key: team_key.clone(),
                    user_id,
                    user_name: user_name.unwrap_or_else(|| "System".into()),
                    is_bot,
                    body: c.body,
                    url,
                    created_at: c.created_at,
                    updated_at: c.updated_at.unwrap_or(c.created_at),
                });
            }
        }
        comments.sort_by_key(|a| a.created_at);

        let mut history = Vec::new();
        if let Some(hist_conn) = self.history {
            for h in hist_conn.nodes {
                let (actor_id, actor_name, is_bot) = resolve_actor(h.actor, h.bot_actor);
                let added_labels = h
                    .added_labels
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| l.name.into())
                    .collect();
                let removed_labels = h
                    .removed_labels
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| l.name.into())
                    .collect();

                history.push(IssueHistoryEntry {
                    id: HistoryId::new(h.id),
                    issue_id: issue_id.clone(),
                    issue_identifier: issue_identifier.clone(),
                    team_key: team_key.clone(),
                    actor_id,
                    actor_name,
                    is_bot,
                    from_state: h.from_state.map(|s| s.name.unwrap_or(s.id).into()),
                    to_state: h.to_state.map(|s| s.name.unwrap_or(s.id).into()),
                    from_priority: h.from_priority.map(IssuePriority::from_linear_int),
                    to_priority: h.to_priority.map(IssuePriority::from_linear_int),
                    from_assignee: h
                        .from_assignee
                        .map(|u| u.display_name.unwrap_or(u.name).into()),
                    to_assignee: h
                        .to_assignee
                        .map(|u| u.display_name.unwrap_or(u.name).into()),
                    from_project: h.from_project.map(|p| p.name.unwrap_or(p.id).into()),
                    to_project: h.to_project.map(|p| p.name.unwrap_or(p.id).into()),
                    from_title: h.from_title.map(Into::into),
                    to_title: h.to_title.map(Into::into),
                    from_parent: h.from_parent.map(|p| p.identifier.into()),
                    to_parent: h.to_parent.map(|p| p.identifier.into()),
                    added_labels,
                    removed_labels,
                    updated_description: h.updated_description.unwrap_or(false),
                    created_at: h.created_at,
                });
            }
        }
        history.sort_by_key(|a| a.created_at);

        IssueActivityPayload {
            issue_id,
            synced_updated_at,
            comments,
            history,
        }
    }
}

fn resolve_actor(
    user: Option<UserNodeDto>,
    bot_actor: Option<BotActorDto>,
) -> (Option<UserId>, Option<CompactString>, bool) {
    if let Some(u) = user {
        (
            Some(UserId::new(u.id)),
            Some(u.display_name.unwrap_or(u.name).into()),
            false,
        )
    } else if let Some(bot) = bot_actor {
        (
            None,
            Some(bot.name.unwrap_or_else(|| "Bot".to_string()).into()),
            true,
        )
    } else {
        (None, Some("System".into()), true)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueUpdateData {
    #[serde(rename = "issueUpdate")]
    pub issue_update: IssueMutationPayloadDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueCreateData {
    #[serde(rename = "issueCreate")]
    pub issue_create: IssueMutationPayloadDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueMutationPayloadDto {
    pub success: bool,
    pub issue: Option<IssueNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentCreateData {
    #[serde(rename = "commentCreate")]
    pub comment_create: CommentMutationPayloadDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentMutationPayloadDto {
    pub success: bool,
    pub comment: Option<CommentNodeDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueLabelCreateData {
    #[serde(rename = "issueLabelCreate")]
    pub issue_label_create: IssueLabelMutationPayloadDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueLabelMutationPayloadDto {
    pub success: bool,
    #[serde(rename = "issueLabel")]
    pub issue_label: Option<LabelNodeDto>,
}

impl CommentNodeDto {
    pub fn into_domain(
        self,
        issue_id: IssueId,
        issue_identifier: CompactString,
        team_key: CompactString,
        fallback_url: &str,
    ) -> Comment {
        let (user_id, user_name, is_bot) = resolve_actor(self.user, self.bot_actor);
        let url = self
            .url
            .unwrap_or_else(|| fallback_url.to_string())
            .into();
        Comment {
            id: CommentId::new(self.id),
            issue_id,
            issue_identifier,
            team_key,
            user_id,
            user_name: user_name.unwrap_or_else(|| "System".into()),
            is_bot,
            body: self.body,
            url,
            created_at: self.created_at,
            updated_at: self.updated_at.unwrap_or(self.created_at),
        }
    }
}

