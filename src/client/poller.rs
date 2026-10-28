use std::{
    sync::{Arc, mpsc::Sender},
    time::Duration,
};

use tokio::sync::{Mutex, Semaphore, mpsc as tokio_mpsc, watch};

use crate::{
    client::{config::ClientConfig, error::ClientError, service::LinearService},
    config::LlimConfig,
    dispatcher::Dispatcher,
    domain::{Issue, LinearMutation},
    event::LlimEvent,
    id::{IssueId, ProjectId},
    result::LlimError,
};

#[derive(Debug, Clone)]
pub enum PollerCommand {
    SyncNow,
    PromoteIssue(IssueId),
    EnqueueBackfill(Vec<Issue>),
    RefreshProjects(Vec<ProjectId>),
    ExecuteMutation(Box<LinearMutation>),
}

#[derive(Debug, Clone)]
pub struct LinearPollerHandle {
    cmd_tx: tokio_mpsc::UnboundedSender<PollerCommand>,
}

impl LinearPollerHandle {
    pub fn sync_now(&self) {
        let _ = self.cmd_tx.send(PollerCommand::SyncNow);
    }

    pub fn promote_issue(&self, issue_id: IssueId) {
        let _ = self.cmd_tx.send(PollerCommand::PromoteIssue(issue_id));
    }

    pub fn enqueue_backfill(&self, issues: Vec<Issue>) {
        if !issues.is_empty() {
            let _ = self.cmd_tx.send(PollerCommand::EnqueueBackfill(issues));
        }
    }

    pub fn refresh_projects(&self, project_ids: Vec<ProjectId>) {
        if !project_ids.is_empty() {
            let _ = self
                .cmd_tx
                .send(PollerCommand::RefreshProjects(project_ids));
        }
    }

    pub fn execute_mutation(&self, mutation: LinearMutation) {
        let _ = self
            .cmd_tx
            .send(PollerCommand::ExecuteMutation(Box::new(mutation)));
    }
}

pub fn spawn_linear_poller(
    service: LinearService,
    sender: Sender<LlimEvent>,
    mut config_rx: watch::Receiver<LlimConfig>,
    offline: bool,
) -> LinearPollerHandle {
    let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<PollerCommand>();
    let service = Arc::new(Mutex::new(service));
    let sync_gate = Arc::new(Semaphore::new(1));

    tokio::spawn(async move {
        let initial_cfg = config_rx.borrow().clone();
        let mut delta_secs = initial_cfg.polling.delta_poll_seconds.max(20);
        let mut reconcile_mins = initial_cfg.polling.full_reconcile_minutes.max(1);

        let mut delta_ticker = tokio::time::interval(Duration::from_secs(delta_secs));
        delta_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        delta_ticker.tick().await;

        let mut reconcile_ticker = tokio::time::interval(Duration::from_secs(reconcile_mins * 60));
        reconcile_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reconcile_ticker.tick().await;

        let mut backfill_ticker = tokio::time::interval(Duration::from_secs(6));
        backfill_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        backfill_ticker.tick().await;

        // Kick off initial sync immediately if not offline
        if !offline {
            run_sync_guarded(&service, &sync_gate, &sender).await;
        }

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        PollerCommand::SyncNow => {
                            run_sync_guarded(&service, &sync_gate, &sender).await;
                        }
                        PollerCommand::PromoteIssue(issue_id) => {
                            let mut svc = service.lock().await;
                            svc.promote_issue(issue_id);
                            if let Err(err) = svc.step_backfill(&sender).await {
                                handle_client_error(err, &sender);
                            }
                        }
                        PollerCommand::EnqueueBackfill(issues) => {
                            let mut svc = service.lock().await;
                            svc.enqueue_issues(&issues, None);
                        }
                        PollerCommand::RefreshProjects(pids) => {
                            let mut svc = service.lock().await;
                            if let Err(err) = svc.refresh_projects(&pids, &sender).await {
                                handle_client_error(err, &sender);
                            }
                        }
                        PollerCommand::ExecuteMutation(mutation) => {
                            let mut svc = service.lock().await;
                            if let Err(err) = svc.execute_mutation(*mutation, &sender).await {
                                sender.dispatch(LlimEvent::MutationFailed(err.to_string().into()));
                            }
                        }
                    }
                }
                _ = delta_ticker.tick() => {
                    if !offline {
                        run_sync_guarded(&service, &sync_gate, &sender).await;
                    }
                }
                _ = reconcile_ticker.tick() => {
                    if !offline {
                        if let Ok(_permit) = sync_gate.try_acquire() {
                            let mut svc = service.lock().await;
                            if let Err(err) = svc.reconcile(&sender).await {
                                handle_client_error(err, &sender);
                            }
                        }
                    }
                }
                _ = backfill_ticker.tick() => {
                    if !offline {
                        if let Ok(_permit) = sync_gate.try_acquire() {
                            let mut svc = service.lock().await;
                            if let Err(err) = svc.step_backfill(&sender).await {
                                handle_client_error(err, &sender);
                            }
                        }
                    }
                }
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let next_cfg = config_rx.borrow_and_update().clone();
                    let next_delta = next_cfg.polling.delta_poll_seconds.max(20);
                    let next_reconcile = next_cfg.polling.full_reconcile_minutes.max(1);

                    {
                        let mut svc = service.lock().await;
                        svc.update_config(ClientConfig::from_llim_config(&next_cfg, offline));
                    }

                    if next_delta != delta_secs {
                        delta_secs = next_delta;
                        delta_ticker = tokio::time::interval(Duration::from_secs(delta_secs));
                        delta_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        delta_ticker.tick().await;
                    }
                    if next_reconcile != reconcile_mins {
                        reconcile_mins = next_reconcile;
                        reconcile_ticker = tokio::time::interval(Duration::from_secs(reconcile_mins * 60));
                        reconcile_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        reconcile_ticker.tick().await;
                    }

                    if !offline {
                        run_sync_guarded(&service, &sync_gate, &sender).await;
                    }
                }
            }
        }
    });

    LinearPollerHandle { cmd_tx }
}

async fn run_sync_guarded(
    service: &Arc<Mutex<LinearService>>,
    sync_gate: &Arc<Semaphore>,
    sender: &Sender<LlimEvent>,
) {
    let Ok(_permit) = sync_gate.try_acquire() else {
        return;
    };
    let mut svc = service.lock().await;
    if let Err(err) = svc.sync(sender).await {
        handle_client_error(err, sender);
    }
}

fn handle_client_error(err: ClientError, sender: &Sender<LlimEvent>) {
    match err {
        ClientError::OfflineMode => {}
        ClientError::Authentication => {
            sender.dispatch(LlimEvent::AppError(LlimError::InvalidLinearToken));
        }
        other => {
            sender.dispatch(LlimEvent::SyncError(other.to_string().into()));
        }
    }
}
