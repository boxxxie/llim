use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc::Sender,
    time::Duration,
};

use compact_str::CompactString;
use tokio::sync::watch;

use crate::{
    config::{LlimConfig, NotificationsSection},
    dispatcher::Dispatcher,
    domain::DomainChange,
    event::LlimEvent,
    id::{IssueId, UserId},
    os_notify::{send_os_notification, NotifyUrgency},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedNotification {
    pub summary: CompactString,
    pub body: CompactString,
    pub urgency: NotifyUrgency,
    pub url: Option<CompactString>,
}

/// Manages queued `DomainChange` events and evaluates them when the single periodic
/// Tokio timer dispatches `LlimEvent::FlushNotificationBuffer`.
#[derive(Debug, Default)]
pub struct NotificationBuffer {
    queued: Vec<DomainChange>,
}

impl NotificationBuffer {
    pub fn new() -> Self {
        Self { queued: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queued.len()
    }

    /// Pre-filters and appends incoming domain changes into the buffer.
    pub fn enqueue(
        &mut self,
        changes: impl IntoIterator<Item = DomainChange>,
        viewer_id: Option<&UserId>,
        config: &NotificationsSection,
    ) {
        for change in changes {
            // Silent metadata diffs without an actor only go to the Activity Feed, not OS alerts.
            if change.actor_id.is_none() && change.actor_name.is_none() {
                continue;
            }
            if config.ignore_bot_events && change.is_bot {
                continue;
            }
            if config.ignore_self_events {
                if let (Some(actor), Some(viewer)) = (change.actor_id.as_ref(), viewer_id) {
                    if actor == viewer {
                        continue;
                    }
                }
            }
            self.queued.push(change);
        }
    }

    /// Evaluates buffered changes according to `[notifications]` config and returns the
    /// resulting notification payloads (while optionally dispatching `notify-send`).
    pub fn evaluate_and_flush(
        &mut self,
        config: &NotificationsSection,
        send_desktop: bool,
    ) -> Vec<EmittedNotification> {
        if self.queued.is_empty() {
            return Vec::new();
        }

        let drained = std::mem::take(&mut self.queued);

        // Group by IssueId first so a single issue never emits split notifications in the same flush.
        let mut by_issue: BTreeMap<IssueId, Vec<DomainChange>> = BTreeMap::new();
        for change in drained {
            by_issue
                .entry(change.issue_id.clone())
                .or_default()
                .push(change);
        }

        let mut emitted = Vec::new();
        let mut standard_groups: Vec<(IssueId, Vec<DomainChange>)> = Vec::new();

        for (issue_id, group) in by_issue {
            let has_direct_attention = config.direct_attention_alert
                && group.iter().any(|c| c.is_direct_attention);

            if has_direct_attention {
                // Strategy A: Direct Attention — combine all changes for this issue into 1 dedicated alert.
                let first = &group[0];
                let summaries: Vec<&str> = group.iter().map(|c| c.summary.as_str()).collect();
                let combined_summary = deduplicate_summaries(&summaries);
                emitted.push(EmittedNotification {
                    summary: format!(
                        "[{}] {} — {}",
                        first.team_key, first.issue_identifier, first.issue_title
                    )
                    .into(),
                    body: combined_summary.into(),
                    urgency: NotifyUrgency::Critical,
                    url: Some(first.url.clone()),
                });
            } else {
                standard_groups.push((issue_id, group));
            }
        }

        let coalesce_threshold = config.coalesce_threshold.max(1);
        if standard_groups.len() > coalesce_threshold {
            // Strategy C: Multi-Issue Digest Summary
            let issue_count = standard_groups.len();
            let teams: BTreeSet<&str> = standard_groups
                .iter()
                .filter_map(|(_, g)| g.first().map(|c| c.team_key.as_str()))
                .collect();
            let projects: BTreeSet<&str> = standard_groups
                .iter()
                .flat_map(|(_, g)| g.iter().filter_map(|c| c.project_name.as_deref()))
                .collect();

            let team_label = if teams.is_empty() {
                "Linear".to_string()
            } else {
                teams.into_iter().collect::<Vec<_>>().join(", ")
            };
            let summary = if projects.is_empty() {
                format!("{issue_count} issues updated in {team_label}")
            } else {
                let proj_list = projects.into_iter().take(3).collect::<Vec<_>>().join(" & ");
                format!("{issue_count} issues updated across {team_label} ({proj_list})")
            };

            let identifiers: Vec<&str> = standard_groups
                .iter()
                .filter_map(|(_, g)| g.first().map(|c| c.issue_identifier.as_str()))
                .take(8)
                .collect();

            emitted.push(EmittedNotification {
                summary: summary.into(),
                body: format!("Updated: {}", identifiers.join(", ")).into(),
                urgency: NotifyUrgency::Normal,
                url: None,
            });
        } else {
            // Strategy B: Per-Issue Multi-Edit Deduplication
            for (_issue_id, group) in standard_groups {
                if config.per_issue_dedup {
                    let first = &group[0];
                    let summaries: Vec<&str> = group.iter().map(|c| c.summary.as_str()).collect();
                    let combined_summary = deduplicate_summaries(&summaries);
                    emitted.push(EmittedNotification {
                        summary: format!(
                            "{} updated: {}",
                            first.issue_identifier, first.issue_title
                        )
                        .into(),
                        body: combined_summary.into(),
                        urgency: NotifyUrgency::Normal,
                        url: Some(first.url.clone()),
                    });
                } else {
                    for change in group {
                        emitted.push(EmittedNotification {
                            summary: format!(
                                "{} updated: {}",
                                change.issue_identifier, change.issue_title
                            )
                            .into(),
                            body: change.summary,
                            urgency: NotifyUrgency::Normal,
                            url: Some(change.url),
                        });
                    }
                }
            }
        }

        if send_desktop && config.desktop_notifications_enabled {
            for alert in &emitted {
                send_os_notification(
                    alert.summary.as_str(),
                    alert.body.as_str(),
                    alert.urgency,
                    6000,
                    alert.url.as_deref(),
                );
            }
        }

        emitted
    }
}

fn deduplicate_summaries(summaries: &[&str]) -> String {
    let mut unique: Vec<&str> = Vec::new();
    for &s in summaries {
        if !unique.contains(&s) {
            unique.push(s);
        }
    }
    unique.join("; ")
}

/// Spawns the single independent Tokio timer that emits `LlimEvent::FlushNotificationBuffer`
/// every `notification_check_seconds`, dynamically updating its interval when `LlimConfig` changes.
pub fn spawn_notification_buffer_timer(
    sender: Sender<LlimEvent>,
    mut config_rx: watch::Receiver<LlimConfig>,
) {
    tokio::spawn(async move {
        let mut current_secs = config_rx
            .borrow()
            .notifications
            .notification_check_seconds
            .max(2);
        let mut ticker = tokio::time::interval(Duration::from_secs(current_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Consume the immediate first tick
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    sender.dispatch(LlimEvent::FlushNotificationBuffer);
                }
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let next_secs = config_rx
                        .borrow_and_update()
                        .notifications
                        .notification_check_seconds
                        .max(2);
                    if next_secs != current_secs {
                        current_secs = next_secs;
                        ticker = tokio::time::interval(Duration::from_secs(current_secs));
                        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        ticker.tick().await;
                    }
                }
            }
        }
    });
}
