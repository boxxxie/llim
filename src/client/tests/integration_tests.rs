use std::{process::Command, sync::mpsc};

use chrono::{TimeZone, Utc};
use serde_json::json;
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::{
    client::{
        backfill::BackfillPriorityQueue, config::ClientConfig, error::ClientError,
        service::LinearService,
    },
    config::{DatabaseSection, LlimConfig, NotificationsSection},
    domain::{
        AttentionReason, DomainChange, HydrationState, Issue, IssuePriority, RateLimitStatus,
        StateCategory,
    },
    event::LlimEvent,
    id::{IssueId, ProjectId, StateId, TeamId, UserId},
    notification_buffer::NotificationBuffer,
    os_notify::NotifyUrgency,
    stores::{LinearStore, resolve_duckdb_binary},
};

fn sample_issue(
    id: &str,
    identifier: &str,
    priority: IssuePriority,
    category: StateCategory,
    assignee_id: Option<&str>,
    updated_secs: i64,
) -> Issue {
    Issue {
        id: IssueId::new(id),
        identifier: identifier.into(),
        title: format!("Title for {identifier}").into(),
        description: Some("Detailed markdown description".to_string()),
        team_id: TeamId::new("team-acme"),
        team_key: "ACM".into(),
        team_name: "acme".into(),
        project_id: Some(ProjectId::new("proj-1")),
        project_name: Some("Core Platform".into()),
        state_id: StateId::new("state-1"),
        state_name: "In Progress".into(),
        state_color: "#fabd2f".into(),
        state_category: category,
        priority,
        assignee_id: assignee_id.map(UserId::new),
        assignee_name: assignee_id.map(|_| "Alice".into()),
        creator_id: Some(UserId::new("user-other")),
        creator_name: Some("Alice".into()),
        labels: vec!["backend".into()],
        label_ids: vec![crate::id::LabelId::new("lbl-backend")],
        blocked_by: Vec::new(),
        has_unacknowledged_mention: false,
        git_branch_name: Some(format!("feat/{}", identifier.to_ascii_lowercase()).into()),
        url: format!("https://linear.app/acme/issue/{identifier}").into(),
        created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        updated_at: Utc.timestamp_opt(updated_secs, 0).unwrap(),
        completed_at: None,
        canceled_at: None,
        archived_at: None,
        details_synced_at: None,
    }
}

#[test]
fn test_backfill_priority_queue_ordering_and_tier_zero_preemption() {
    let viewer = UserId::new("viewer-1");
    let mut queue = BackfillPriorityQueue::new();

    // Tier 4: Completed historical issue
    let closed_issue = sample_issue(
        "iss-closed",
        "ACM-1",
        IssuePriority::High,
        StateCategory::Completed,
        Some("viewer-1"),
        1_700_000_500,
    );
    // Tier 3: Active unassigned normal priority issue
    let normal_active = sample_issue(
        "iss-normal",
        "ACM-2",
        IssuePriority::Medium,
        StateCategory::Unstarted,
        None,
        1_700_000_600,
    );
    // Tier 2: Active issue assigned to viewer
    let personal_active = sample_issue(
        "iss-personal",
        "ACM-3",
        IssuePriority::High,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_000_400,
    );

    assert!(queue.enqueue_issue(&closed_issue, Some(&viewer), None));
    assert!(queue.enqueue_issue(&normal_active, Some(&viewer), None));
    assert!(queue.enqueue_issue(&personal_active, Some(&viewer), None));
    // Duplicate enqueue is ignored
    assert!(!queue.enqueue_issue(&personal_active, Some(&viewer), None));

    // Promote closed issue to Tier 0 (user opened IssueDetailsPopup)
    queue.promote_to_tier_zero(IssueId::new("iss-closed"));

    // Even when rate limit is low (<30%), Tier 0 is still allowed to pop!
    let low_rl = RateLimitStatus {
        requests_remaining: Some(200),
        requests_limit: Some(1500),
        complexity_remaining: Some(200_000),
        complexity_limit: Some(250_000),
        reset_epoch_secs: None,
    };
    let tier0_only = queue.pop_batch(10, Some(&low_rl));
    assert_eq!(tier0_only, vec![IssueId::new("iss-closed")]);

    // With healthy rate limit, Tier 2 pops before Tier 3
    let healthy_rl = RateLimitStatus::default();
    let rest = queue.pop_batch(10, Some(&healthy_rl));
    assert_eq!(
        rest,
        vec![IssueId::new("iss-personal"), IssueId::new("iss-normal")]
    );
    assert!(queue.is_empty());
}

#[test]
fn test_notification_buffer_strategies_and_issue_grouping() {
    let viewer = UserId::new("viewer-1");
    let cfg = NotificationsSection {
        notification_check_seconds: 10,
        direct_attention_alert: true,
        per_issue_dedup: true,
        coalesce_threshold: 2,
        ignore_self_events: true,
        ignore_bot_events: true,
        desktop_notifications_enabled: false,
    };

    let mut buf = NotificationBuffer::new();

    // 1. Self event and bot event should be filtered out on enqueue
    let self_change = DomainChange {
        issue_id: IssueId::new("iss-self"),
        issue_identifier: "ACM-10".into(),
        issue_title: "Self edit".into(),
        team_key: "ACM".into(),
        project_name: None,
        actor_id: Some(viewer.clone()),
        actor_name: Some("Alice".into()),
        is_bot: false,
        is_direct_attention: true,
        summary: "State -> Done".into(),
        url: "https://linear.app/acme/issue/ACM-10".into(),
        timestamp: Utc::now(),
    };
    let bot_change = DomainChange {
        issue_id: IssueId::new("iss-bot"),
        issue_identifier: "ACM-11".into(),
        issue_title: "Bot edit".into(),
        team_key: "ACM".into(),
        project_name: None,
        actor_id: None,
        actor_name: Some("GitHub".into()),
        is_bot: true,
        is_direct_attention: false,
        summary: "Linked PR".into(),
        url: "https://linear.app/acme/issue/ACM-11".into(),
        timestamp: Utc::now(),
    };
    buf.enqueue([self_change, bot_change], Some(&viewer), &cfg);
    assert!(buf.is_empty());

    // 2. Single issue with both a standard edit AND a direct @mention in the same window
    // must group by IssueId and emit exactly 1 Critical notification (no duplicate standard alert)
    let c1 = DomainChange {
        issue_id: IssueId::new("iss-1"),
        issue_identifier: "ACM-101".into(),
        issue_title: "Fix Quack server".into(),
        team_key: "ACM".into(),
        project_name: Some("Storage".into()),
        actor_id: Some(UserId::new("alice")),
        actor_name: Some("Alice".into()),
        is_bot: false,
        is_direct_attention: false,
        summary: "State: Todo -> In Progress".into(),
        url: "https://linear.app/acme/issue/ACM-101".into(),
        timestamp: Utc::now(),
    };
    let c2 = DomainChange {
        issue_id: IssueId::new("iss-1"),
        issue_identifier: "ACM-101".into(),
        issue_title: "Fix Quack server".into(),
        team_key: "ACM".into(),
        project_name: Some("Storage".into()),
        actor_id: Some(UserId::new("alice")),
        actor_name: Some("Alice".into()),
        is_bot: false,
        is_direct_attention: true,
        summary: "@mentioned you: Can you check port 9494?".into(),
        url: "https://linear.app/acme/issue/ACM-101".into(),
        timestamp: Utc::now(),
    };
    buf.enqueue([c1, c2], Some(&viewer), &cfg);
    let emitted = buf.evaluate_and_flush(&cfg, false);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].urgency, NotifyUrgency::Critical);
    assert!(emitted[0].body.contains("State: Todo -> In Progress"));
    assert!(emitted[0].body.contains("@mentioned you"));

    // 3. Three standard issues (> coalesce_threshold=2) coalesce into 1 digest notification
    for idx in 1..=3 {
        buf.enqueue(
            [DomainChange {
                issue_id: IssueId::new(format!("iss-std-{idx}")),
                issue_identifier: format!("PLT-{idx}").into(),
                issue_title: format!("Task {idx}").into(),
                team_key: "PLT".into(),
                project_name: Some("Q4 Roadmap".into()),
                actor_id: Some(UserId::new("bob")),
                actor_name: Some("Bob".into()),
                is_bot: false,
                is_direct_attention: false,
                summary: "Updated priority".into(),
                url: format!("https://linear.app/platform/issue/PLT-{idx}").into(),
                timestamp: Utc::now(),
            }],
            Some(&viewer),
            &cfg,
        );
    }
    let digest = buf.evaluate_and_flush(&cfg, false);
    assert_eq!(digest.len(), 1);
    assert!(digest[0].summary.contains("3 issues updated"));
    assert!(digest[0].body.contains("PLT-1, PLT-2, PLT-3"));
}

#[test]
fn test_attention_reasons_computation() {
    let viewer = UserId::new("viewer-1");
    let mut issue = sample_issue(
        "iss-att",
        "ACM-42",
        IssuePriority::Urgent,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_001_000,
    );
    issue.has_unacknowledged_mention = true;
    issue.blocked_by.push("ACM-10".into());

    let reasons = issue.attention_reasons(Some(&viewer));
    assert_eq!(
        reasons,
        vec![
            AttentionReason::Mention,
            AttentionReason::Blocked,
            AttentionReason::Urgent,
            AttentionReason::InProgress,
        ]
    );
}

#[tokio::test]
async fn test_wiremock_bootstrap_and_two_step_delta_sync() {
    let server = MockServer::start().await;

    let response_body = json!({
        "data": {
            "viewer": {
                "id": "viewer-1",
                "name": "alice",
                "displayName": "Alice",
                "email": "alice@example.com"
            },
            "teams": {
                "nodes": [
                    {
                        "id": "team-acme",
                        "key": "ACM",
                        "name": "acme",
                        "states": {
                            "nodes": [
                                {
                                    "id": "st-started",
                                    "name": "In Progress",
                                    "type": "started",
                                    "color": "#fabd2f"
                                }
                            ]
                        }
                    },
                    {
                        "id": "team-platform",
                        "key": "PLT",
                        "name": "Platform",
                        "states": { "nodes": [] }
                    }
                ]
            },
            "projects": {
                "nodes": [
                    {
                        "id": "proj-1",
                        "name": "Linear TUI",
                        "description": "Read-only TUI monitor",
                        "state": "started",
                        "progress": 0.75,
                        "scope": 12.0,
                        "targetDate": "2026-12-01",
                        "updatedAt": "2026-10-26T10:00:00Z",
                        "archivedAt": null,
                        "url": "https://linear.app/acme/project/linear-tui",
                        "teams": { "nodes": [{ "id": "team-acme", "key": "ACM" }] }
                    }
                ]
            },
            "issues": {
                "pageInfo": { "hasNextPage": false, "endCursor": null },
                "nodes": [
                    {
                        "id": "iss-100",
                        "identifier": "ACM-100",
                        "title": "Build DuckDB 2.0 Quack storage",
                        "description": "Use DuckDB 2.0 with quack_serve",
                        "url": "https://linear.app/acme/issue/ACM-100",
                        "branchName": "feat/acm-100-duckdb-quack",
                        "priority": 1,
                        "createdAt": "2026-10-25T08:00:00Z",
                        "updatedAt": "2026-10-26T12:00:00Z",
                        "completedAt": null,
                        "canceledAt": null,
                        "archivedAt": null,
                        "team": { "id": "team-acme", "key": "ACM", "name": "acme" },
                        "project": { "id": "proj-1", "name": "Linear TUI" },
                        "state": { "id": "st-started", "name": "In Progress", "type": "started", "color": "#fabd2f" },
                        "assignee": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                        "creator": { "id": "user-2", "name": "alice", "displayName": "Alice" },
                        "labels": { "nodes": [{ "id": "lbl-1", "name": "rust" }] },
                        "inverseRelations": { "nodes": [] },
                        "comments": {
                            "nodes": [
                                {
                                    "id": "cmt-1",
                                    "body": "Hey @alice this looks awesome!",
                                    "createdAt": "2026-10-26T12:01:00Z",
                                    "updatedAt": "2026-10-26T12:01:00Z",
                                    "url": "https://linear.app/acme/issue/ACM-100#comment-1",
                                    "user": { "id": "user-2", "name": "alice", "displayName": "Alice" },
                                    "botActor": null
                                }
                            ]
                        },
                        "history": {
                            "nodes": [
                                {
                                    "id": "hist-1",
                                    "createdAt": "2026-10-26T11:55:00Z",
                                    "updatedDescription": false,
                                    "actor": { "id": "user-2", "name": "alice", "displayName": "Alice" },
                                    "botActor": null,
                                    "fromState": { "id": "st-todo", "name": "Todo" },
                                    "toState": { "id": "st-started", "name": "In Progress" },
                                    "fromAssignee": null,
                                    "toAssignee": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                                    "fromPriority": 2,
                                    "toPriority": 1,
                                    "fromProject": null,
                                    "toProject": { "id": "proj-1", "name": "Linear TUI" },
                                    "addedLabels": [{ "id": "lbl-1", "name": "rust" }],
                                    "removedLabels": []
                                }
                            ]
                        }
                    }
                ]
            }
        }
    });

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("date", "Mon, 26 Oct 2026 12:05:00 GMT")
                .insert_header("x-ratelimit-requests-remaining", "1420")
                .insert_header("x-ratelimit-requests-limit", "1500")
                .insert_header("x-ratelimit-complexity-remaining", "240000")
                .insert_header("x-ratelimit-complexity-limit", "250000")
                .set_body_json(response_body),
        )
        .mount(&server)
        .await;

    let mut llim_cfg = LlimConfig::default();
    llim_cfg.linear.api_key = Some("lin_api_test_key".into());
    llim_cfg.linear.teams = vec!["acme".into(), "Platform".into()];

    let client_cfg = ClientConfig::from_llim_config(&llim_cfg, false)
        .with_endpoint(format!("{}/graphql", server.uri()));

    let mut service = LinearService::new(client_cfg, None).unwrap();
    let (tx, rx) = mpsc::channel();

    // 1. Cold bootstrap sync
    service.sync(&tx).await.unwrap();

    let mut hydrated = false;
    let mut details_loaded = false;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            LlimEvent::InitialHydrationLoaded(payload) => {
                assert_eq!(payload.teams.len(), 2);
                assert_eq!(payload.projects.len(), 1);
                assert_eq!(payload.issues.len(), 1);
                assert_eq!(payload.issues[0].identifier, "ACM-100");
                hydrated = true;
            }
            LlimEvent::IssueDetailsLoaded(act) => {
                assert_eq!(act.issue_id, IssueId::new("iss-100"));
                assert_eq!(act.comments.len(), 1);
                assert_eq!(act.history.len(), 1);
                details_loaded = true;
            }
            _ => {}
        }
    }
    assert!(hydrated);
    assert!(details_loaded);

    // 2. Subsequent sync uses delta poll + two-step activity batch query
    service.sync(&tx).await.unwrap();
    let mut delta_loaded = false;
    while let Ok(ev) = rx.try_recv() {
        if let LlimEvent::DeltaBatchLoaded(delta) = ev {
            assert_eq!(delta.issues.len(), 1);
            assert_eq!(delta.comments_by_issue.len(), 1);
            assert_eq!(delta.history_by_issue.len(), 1);
            delta_loaded = true;
        }
    }
    assert!(delta_loaded);
}

#[tokio::test]
async fn test_offline_mode_blocks_network_calls() {
    let cfg = LlimConfig::default();
    let client_cfg = ClientConfig::from_llim_config(&cfg, true);
    let mut service = LinearService::new(client_cfg, None).unwrap();
    let (tx, _rx) = mpsc::channel();
    let res = service.sync(&tx).await;
    assert_eq!(res, Err(ClientError::OfflineMode));
}

#[test]
fn test_duckdb_persistence_and_quack_roundtrip() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_llim.duckdb");

    // Pick a high ephemeral port for testing Quack server concurrently
    let test_port = 19594;
    let db_cfg = DatabaseSection {
        quack_server_enabled: true,
        quack_port: test_port,
    };

    let mut store = LinearStore::new(db_path.clone());
    store.init_duckdb_and_load(&db_cfg, None);

    let issue = sample_issue(
        "iss-duck-1",
        "ACM-777",
        IssuePriority::Urgent,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_050_000,
    );

    let bootstrap = crate::event::BootstrapPayload {
        viewer: None,
        resolved_team_keys: vec!["ACM".into()],
        unmatched_teams: Vec::new(),
        teams: vec![crate::domain::Team {
            id: TeamId::new("team-acme"),
            key: "ACM".into(),
            name: "acme".into(),
        }],
        states: Vec::new(),
        users: Vec::new(),
        labels: Vec::new(),
        projects: Vec::new(),
        issues: vec![issue],
        server_time: Utc::now(),
    };

    store.apply_bootstrap(bootstrap, None);
    assert_eq!(store.hydration_state, HydrationState::LiveSteadyState);

    // If Quack server is listening, verify a separate DuckDB 2.0 CLI client can attach and query live!
    if let Some(uri) = store.quack_uri() {
        let token = std::fs::read_to_string(crate::config::default_quack_token_path())
            .unwrap_or_default()
            .trim()
            .to_string();
        let duckdb_bin = resolve_duckdb_binary();
        let query_sql = format!(
            "LOAD quack; CREATE SECRET (TYPE quack, TOKEN '{token}'); ATTACH '{uri}' AS llim; SELECT identifier FROM llim.issues WHERE id = 'iss-duck-1';"
        );
        let out = Command::new(&duckdb_bin)
            .arg("-dark-mode")
            .arg("-batch")
            .arg("-json")
            .arg("-c")
            .arg(&query_sql)
            .output()
            .expect("duckdb client query");
        if out.status.success() {
            let stdout_str = String::from_utf8_lossy(&out.stdout);
            assert!(stdout_str.contains("ACM-777"));
        }
    }

    store.shutdown();
    std::thread::sleep(std::time::Duration::from_millis(250));

    // Reload from disk in a fresh LinearStore instance and verify ACM-777 was persisted
    let mut store2 = LinearStore::new(db_path);
    let disabled_quack = DatabaseSection {
        quack_server_enabled: false,
        quack_port: test_port,
    };
    store2.init_duckdb_and_load(&disabled_quack, None);
    assert_eq!(store2.issues.len(), 1);
    assert_eq!(
        store2.issues.get(&IssueId::new("iss-duck-1")).unwrap().identifier,
        "ACM-777"
    );
    store2.shutdown();
}

#[test]
fn test_newest_on_top_default_sort_and_inspector_sync() {
    use crate::{
        domain::{IssueSortOrder, ViewKind},
        ui::stateful_widgets::StatefulWidgets,
    };

    assert_eq!(IssueSortOrder::default(), IssueSortOrder::UpdatedDesc);
    assert_eq!(IssueSortOrder::UpdatedDesc.next(), IssueSortOrder::CreatedDesc);
    assert_eq!(ViewKind::Issues.next(), ViewKind::Attention);
    assert_eq!(ViewKind::Attention.next(), ViewKind::Activity);
    assert_eq!(ViewKind::Activity.next(), ViewKind::Issues);

    let dir = tempdir().unwrap();
    let mut store = LinearStore::new(dir.path().join("sort_test.duckdb"));
    let older = sample_issue(
        "iss-old",
        "ACM-1",
        IssuePriority::Urgent,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_000_100,
    );
    let newer = sample_issue(
        "iss-new",
        "ACM-2",
        IssuePriority::Low,
        StateCategory::Backlog,
        Some("viewer-1"),
        1_700_099_999,
    );
    store.issues.insert(older.id.clone(), older);
    store.issues.insert(newer.id.clone(), newer);

    let cfg = LlimConfig::default();
    let mut ui = StatefulWidgets::new(&cfg);
    ui.recompute_projections(&store);

    // Default sort is newest updated on top (ACM-2 before ACM-1, even though ACM-1 is Urgent)
    assert_eq!(
        ui.issues_view.selected_issue().map(|i| &i.id),
        Some(&IssueId::new("iss-new"))
    );

    // Syncing the bottom 50% inspector automatically selects ACM-2 and requests Tier 0 backfill
    let needs_backfill = ui.sync_inspector(&store);
    assert_eq!(needs_backfill, Some(IssueId::new("iss-new")));
    assert_eq!(
        ui.inspector
            .issue
            .as_ref()
            .map(|i| i.identifier.as_ref()),
        Some("ACM-2")
    );

    // Shift + Down / Shift + Up act as PageDown / PageUp
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut events = Vec::new();
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)),
        &mut events,
    );
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)),
        &mut events,
    );
    assert!(matches!(
        events.as_slice(),
        [LlimEvent::CursorPageDown, LlimEvent::CursorPageUp]
    ));
}

#[tokio::test]
async fn test_write_mutations_issue_update_create_comment_and_labels() {
    use crate::{
        domain::{LinearMutation, WorkflowState},
        id::LabelId,
        ui::popup::write_popup::{
            CommentComposePopup, IssueFormPopup, LabelPickerPopup, QuickPickerPopup,
        },
    };

    let server = MockServer::start().await;
    let mutation_response = json!({
        "data": {
            "issueUpdate": {
                "success": true,
                "issue": {
                    "id": "iss-100",
                    "identifier": "ACM-100",
                    "title": "Updated title via write support",
                    "description": "Updated markdown description",
                    "url": "https://linear.app/acme/issue/ACM-100",
                    "branchName": "feat/acm-100-duckdb-quack",
                    "priority": 1,
                    "createdAt": "2026-10-25T08:00:00Z",
                    "updatedAt": "2026-10-27T02:00:00Z",
                    "completedAt": null,
                    "canceledAt": null,
                    "archivedAt": null,
                    "team": { "id": "team-acme", "key": "ACM", "name": "acme" },
                    "project": { "id": "proj-1", "name": "Linear TUI" },
                    "state": { "id": "st-done", "name": "Done", "type": "completed", "color": "#b8bb26" },
                    "assignee": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                    "creator": { "id": "user-2", "name": "alice", "displayName": "Alice" },
                    "labels": { "nodes": [{ "id": "lbl-new", "name": "write-support" }] },
                    "inverseRelations": { "nodes": [] }
                }
            },
            "issueCreate": {
                "success": true,
                "issue": {
                    "id": "iss-200",
                    "identifier": "ACM-200",
                    "title": "Brand new issue from TUI",
                    "description": "Created in llim",
                    "url": "https://linear.app/acme/issue/ACM-200",
                    "branchName": "feat/acm-200-brand-new",
                    "priority": 2,
                    "createdAt": "2026-10-27T02:01:00Z",
                    "updatedAt": "2026-10-27T02:01:00Z",
                    "completedAt": null,
                    "canceledAt": null,
                    "archivedAt": null,
                    "team": { "id": "team-acme", "key": "ACM", "name": "acme" },
                    "project": null,
                    "state": { "id": "st-started", "name": "In Progress", "type": "started", "color": "#fabd2f" },
                    "assignee": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                    "creator": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                    "labels": { "nodes": [] },
                    "inverseRelations": { "nodes": [] }
                }
            },
            "commentCreate": {
                "success": true,
                "comment": {
                    "id": "cmt-99",
                    "body": "Shipped write support!",
                    "createdAt": "2026-10-27T02:02:00Z",
                    "updatedAt": "2026-10-27T02:02:00Z",
                    "url": "https://linear.app/acme/issue/ACM-100#comment-99",
                    "user": { "id": "viewer-1", "name": "alice", "displayName": "Alice" },
                    "botActor": null
                }
            },
            "issueLabelCreate": {
                "success": true,
                "issueLabel": {
                    "id": "lbl-new",
                    "name": "write-support"
                }
            }
        }
    });

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mutation_response))
        .mount(&server)
        .await;

    let mut llim_cfg = LlimConfig::default();
    llim_cfg.linear.api_key = Some("lin_api_test_key".into());
    let client_cfg = ClientConfig::from_llim_config(&llim_cfg, false)
        .with_endpoint(format!("{}/graphql", server.uri()));
    let mut service = LinearService::new(client_cfg, None).unwrap();
    let (tx, rx) = mpsc::channel();

    let dir = tempdir().unwrap();
    let mut store = LinearStore::new(dir.path().join("write_test.duckdb"));
    store.teams.insert(
        TeamId::new("team-acme"),
        crate::domain::Team {
            id: TeamId::new("team-acme"),
            key: "ACM".into(),
            name: "acme".into(),
        },
    );
    store.states.insert(
        StateId::new("st-done"),
        WorkflowState {
            id: StateId::new("st-done"),
            team_id: TeamId::new("team-acme"),
            name: "Done".into(),
            color: "#b8bb26".into(),
            category: StateCategory::Completed,
        },
    );
    let base_issue = sample_issue(
        "iss-100",
        "ACM-100",
        IssuePriority::Medium,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_000_000,
    );
    store.issues.insert(base_issue.id.clone(), base_issue.clone());
    store.harvest_users_and_labels_from_issues();

    // 1. State picker builds UpdateIssue mutation
    let state_picker = QuickPickerPopup::new_state_picker(&base_issue, &store);
    let state_mut = state_picker.selected_mutation().unwrap();
    service.execute_mutation(state_mut, &tx).await.unwrap();

    // 2. Comment compose popup builds CreateComment mutation
    let mut cmt_popup = CommentComposePopup::new(&base_issue);
    cmt_popup.body = "Shipped write support!".to_string();
    let cmt_mut = cmt_popup.build_mutation().unwrap();
    service.execute_mutation(cmt_mut, &tx).await.unwrap();

    // 3. Label picker builds CreateLabelAndAttach mutation
    let mut lbl_popup = LabelPickerPopup::new(&base_issue, &store);
    lbl_popup.creating_new = true;
    lbl_popup.new_label_name = "write-support".to_string();
    let lbl_mut = lbl_popup.build_mutation().unwrap();
    assert!(matches!(lbl_mut, LinearMutation::CreateLabelAndAttach { .. }));
    service.execute_mutation(lbl_mut, &tx).await.unwrap();

    // 4. IssueFormPopup builds CreateIssue mutation
    let mut create_popup = IssueFormPopup::new_create(&store, Some("ACM"));
    create_popup.title = "Brand new issue from TUI".to_string();
    create_popup.description = "Created in llim".to_string();
    let create_mut = create_popup.build_mutation().unwrap();
    service.execute_mutation(create_mut, &tx).await.unwrap();

    let mut mutated_count = 0;
    let mut comments_created = 0;
    let mut labels_created = 0;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            LlimEvent::IssueMutated {
                issue,
                summary,
                created,
                ..
            } => {
                store.apply_mutated_issue(*issue, &summary, created);
                mutated_count += 1;
            }
            LlimEvent::CommentCreated {
                issue_id,
                comment,
                ..
            } => {
                store.apply_created_comment(&issue_id, comment);
                comments_created += 1;
            }
            LlimEvent::LabelCreated(lbl) => {
                store.apply_created_label(lbl);
                labels_created += 1;
            }
            _ => {}
        }
    }

    assert_eq!(mutated_count, 3);
    assert_eq!(comments_created, 1);
    assert_eq!(labels_created, 1);
    assert!(store.issues.contains_key(&IssueId::new("iss-200")));
    assert_eq!(
        store.comments_by_issue.get(&IssueId::new("iss-100")).unwrap().len(),
        1
    );
    assert!(store.labels.contains_key(&LabelId::new("lbl-new")));
}

#[test]
fn test_bulk_multi_selection_and_left_right_inspector_scroll() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crate::{
        domain::{LinearMutation, WorkflowState},
        ui::{popup::write_popup::QuickPickerPopup, stateful_widgets::StatefulWidgets},
    };

    let dir = tempdir().unwrap();
    let mut store = LinearStore::new(dir.path().join("bulk_test.duckdb"));
    store.states.insert(
        StateId::new("st-acm-done"),
        WorkflowState {
            id: StateId::new("st-acm-done"),
            team_id: TeamId::new("team-acme"),
            name: "Done".into(),
            color: "#b8bb26".into(),
            category: StateCategory::Completed,
        },
    );
    store.states.insert(
        StateId::new("st-plt-done"),
        WorkflowState {
            id: StateId::new("st-plt-done"),
            team_id: TeamId::new("team-platform"),
            name: "Done".into(),
            color: "#b8bb26".into(),
            category: StateCategory::Completed,
        },
    );

    let iss1 = sample_issue(
        "iss-1",
        "ACM-1",
        IssuePriority::High,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_000_200,
    );
    let mut iss2 = sample_issue(
        "iss-2",
        "PLT-2",
        IssuePriority::Low,
        StateCategory::Unstarted,
        Some("viewer-1"),
        1_700_000_100,
    );
    iss2.team_id = TeamId::new("team-platform");
    iss2.team_key = "PLT".into();
    iss2.team_name = "Platform".into();

    store.issues.insert(iss1.id.clone(), iss1);
    store.issues.insert(iss2.id.clone(), iss2);

    let cfg = LlimConfig::default();
    let mut ui = StatefulWidgets::new(&cfg);
    ui.recompute_projections(&store);

    // 1. Left / Right and Shift+Left / Shift+Right scroll the bottom inspector from the top table
    let mut events = Vec::new();
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        &mut events,
    );
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
        &mut events,
    );
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        &mut events,
    );
    ui.dispatch_input(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT)),
        &mut events,
    );
    assert!(matches!(
        events.as_slice(),
        [
            LlimEvent::InspectorScrollDown(3),
            LlimEvent::InspectorScrollUp(3),
            LlimEvent::InspectorScrollDown(10),
            LlimEvent::InspectorScrollUp(10),
        ]
    ));

    // 2. Multi-selection via `toggle_select_current` and `toggle_select_all_visible`
    assert_eq!(ui.target_issues(&store).len(), 1);
    ui.toggle_select_current(&store, true);
    assert_eq!(ui.selected_issue_ids.len(), 1);
    ui.toggle_select_current(&store, false);
    assert_eq!(ui.selected_issue_ids.len(), 2);

    let targets = ui.target_issues(&store);
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].identifier, "ACM-1");
    assert_eq!(targets[1].identifier, "PLT-2");

    // 3. Cross-team bulk state change resolves each issue's team-specific StateId ("st-acm-done" vs "st-plt-done")
    let picker = QuickPickerPopup::new_state_picker_for_targets(targets, &store);
    let batch_mut = picker.selected_mutation().unwrap();
    match batch_mut {
        LinearMutation::Batch {
            mutations,
            summary_label,
        } => {
            assert_eq!(mutations.len(), 2);
            assert!(summary_label.contains("Updated state -> Done on 2 issues"));
            assert!(matches!(
                &mutations[0],
                LinearMutation::UpdateIssue {
                    issue_id,
                    state_id: Some(st),
                    ..
                } if issue_id == &IssueId::new("iss-1") && st == &StateId::new("st-acm-done")
            ));
            assert!(matches!(
                &mutations[1],
                LinearMutation::UpdateIssue {
                    issue_id,
                    state_id: Some(st),
                    ..
                } if issue_id == &IssueId::new("iss-2") && st == &StateId::new("st-plt-done")
            ));
        }
        other => panic!("Expected LinearMutation::Batch, got {other:?}"),
    }

    // Toggle all visible when all are already selected clears them
    ui.toggle_select_all_visible();
    assert!(ui.selected_issue_ids.is_empty());
}

#[test]
fn test_creator_fallback_when_unassigned() {
    let assigned = sample_issue(
        "iss-assigned",
        "ACM-10",
        IssuePriority::High,
        StateCategory::Started,
        Some("viewer-1"),
        1_700_000_000,
    );
    assert_eq!(
        assigned.assignee_or_creator_display(),
        ("Alice".into(), false)
    );

    let unassigned_with_creator = sample_issue(
        "iss-unassigned",
        "ACM-11",
        IssuePriority::Medium,
        StateCategory::Unstarted,
        None,
        1_700_000_000,
    );
    assert_eq!(
        unassigned_with_creator.assignee_or_creator_display(),
        ("Alice (creator)".into(), true)
    );

    let mut unassigned_no_creator = unassigned_with_creator.clone();
    unassigned_no_creator.creator_name = None;
    assert_eq!(
        unassigned_no_creator.assignee_or_creator_display(),
        ("—".into(), false)
    );
}

#[test]
fn test_emacs_keybindings_esc_chords_and_comment_draft_lifecycle() {
    use std::{cell::RefCell, rc::Rc};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use crate::{
        domain::LinearMutation,
        input::{input_processor::InputProcessor, processor::write::CommentComposeProcessor},
        ui::{popup::write_popup::CommentComposePopup, widget::input_field::TextEditor},
    };

    // 1. Verify Emacs / terminal keybindings on TextEditor
    let mut ed = TextEditor::from_text("hello world");
    // Ctrl+a moves to start of line
    ed.handle_key(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL), true);
    assert_eq!(ed.cursor, 0);
    // Ctrl+f moves forward 1 char
    ed.handle_key(&KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL), true);
    assert_eq!(ed.cursor, 1);
    // Ctrl+e moves to end of line
    ed.handle_key(&KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL), true);
    assert_eq!(ed.cursor, 11);
    // Ctrl+w deletes previous word ("world")
    ed.handle_key(&KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL), true);
    assert_eq!(ed.as_str(), "hello ");
    // Ctrl+j and Shift+Enter insert newlines
    ed.handle_key(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL), true);
    ed.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT), true);
    ed.insert_str("line3");
    assert_eq!(ed.as_str(), "hello \n\nline3");
    // Ctrl+a moves to start of "line3", Ctrl+k kills to end of line
    ed.handle_key(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL), true);
    ed.handle_key(&KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL), true);
    assert_eq!(ed.as_str(), "hello \n\n");

    // 2. Verify CommentComposeProcessor draft persistence, Insert -> $EDITOR, Ctrl+x delete draft, Esc+Esc discard, and Esc+Enter save
    let dir = tempdir().unwrap();
    let mut store = LinearStore::new(dir.path().join("draft_test.duckdb"));
    let issue = sample_issue(
        "iss-draft",
        "PLT-88",
        IssuePriority::Medium,
        StateCategory::Started,
        None,
        1_700_000_000,
    );
    store.issues.insert(issue.id.clone(), issue.clone());

    let popup = Rc::new(RefCell::new(CommentComposePopup::new_for_targets_with_draft(
        vec![issue.clone()],
        store.comment_draft(&issue.id),
    )));
    let mut proc = CommentComposeProcessor::new(Rc::clone(&popup));
    let mut events = Vec::new();

    // Type "draft text" -> emits SaveCommentDraft
    for ch in "draft text".chars() {
        proc.apply(
            &LlimEvent::InputKey(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            &mut events,
        );
    }
    for ev in events.drain(..) {
        if let LlimEvent::SaveCommentDraft { issue_id, body } = ev {
            store.set_comment_draft(issue_id, body);
        }
    }
    assert_eq!(store.comment_draft(&issue.id), Some("draft text"));

    // Pressing Insert launches external editor (while Ctrl+e moves cursor to end of line!)
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Insert, KeyModifiers::NONE)),
        &mut events,
    );
    assert!(matches!(
        events.as_slice(),
        [LlimEvent::LaunchExternalEditor { initial_text }] if initial_text == "draft text"
    ));
    events.clear();

    // Reopening CommentComposePopup on the same issue restores the draft
    let reopened = CommentComposePopup::new_for_targets_with_draft(
        vec![issue.clone()],
        store.comment_draft(&issue.id),
    );
    assert!(reopened.is_draft);
    assert_eq!(reopened.body, "draft text");

    // Quick draft deletion with Ctrl+x clears the editor and removes draft from store
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)),
        &mut events,
    );
    assert!(popup.borrow().body.is_empty());
    for ev in events.drain(..) {
        if let LlimEvent::ClearCommentDraft { issue_id, notify } = ev {
            assert!(notify);
            store.clear_comment_draft(&issue_id);
        }
    }
    assert_eq!(store.comment_draft(&issue.id), None);

    // Type again, then test Esc + Esc = don't post to Linear, keep non-blank comment copy as draft & close modal
    for ch in "keep me as draft".chars() {
        proc.apply(
            &LlimEvent::InputKey(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            &mut events,
        );
    }
    events.clear();
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut events,
    );
    assert!(popup.borrow().esc_pending);
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut events,
    );
    assert!(!popup.borrow().esc_pending);
    for ev in &events {
        if let LlimEvent::SaveCommentDraft { issue_id, body } = ev {
            store.set_comment_draft(issue_id.clone(), body.clone());
        }
    }
    assert_eq!(store.comment_draft(&issue.id), Some("keep me as draft"));
    assert!(events.iter().any(|e| matches!(e, LlimEvent::WriteModalClose)));
    events.clear();

    // If the comment copy is blank (e.g. cleared via Backspace/Ctrl+u), Esc + Esc removes the draft
    popup.borrow_mut().clear_draft_text();
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut events,
    );
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut events,
    );
    for ev in events.drain(..) {
        if let LlimEvent::SaveCommentDraft { issue_id, body } = ev {
            store.set_comment_draft(issue_id, body);
        }
    }
    assert_eq!(store.comment_draft(&issue.id), None);

    // Type again, then test Esc + Enter = save/post comment
    for ch in "ship it".chars() {
        proc.apply(
            &LlimEvent::InputKey(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            &mut events,
        );
    }
    events.clear();
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut events,
    );
    proc.apply(
        &LlimEvent::InputKey(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &mut events,
    );
    assert!(events.iter().any(|e| matches!(e, LlimEvent::WriteModalClose)));
    assert!(events.iter().any(|e| matches!(
        e,
        LlimEvent::SubmitMutation(m) if matches!(**m, LinearMutation::CreateComment { ref body, .. } if body == "ship it")
    )));
}



