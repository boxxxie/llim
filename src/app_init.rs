use std::{path::PathBuf, sync::mpsc::Sender};

use tokio::sync::watch;

use crate::{
    client::{config::ClientConfig, poller::spawn_linear_poller, service::LinearService},
    config::{LlimConfig, default_db_path, load_config, resolve_api_key},
    dispatcher::Dispatcher,
    domain::{CURRENT_CACHE_SCHEMA_VERSION, HydrationState, Issue},
    event::LlimEvent,
    llim_app::LlimApp,
    notification_buffer::spawn_notification_buffer_timer,
    result::{LlimError, Result},
    stores::LinearStore,
};

pub fn initialize_app(
    config_path: PathBuf,
    sender: Sender<LlimEvent>,
    offline: bool,
) -> Result<LlimApp> {
    let config: LlimConfig = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            sender.dispatch(LlimEvent::AppError(err));
            LlimConfig::default()
        }
    };

    let mut store = LinearStore::new(default_db_path());
    store.init_duckdb_and_load(&config.database, Some(&sender));

    let has_cached = !store.issues.is_empty()
        && store.sync_meta.issue_watermark.is_some()
        && store.sync_meta.cache_schema_version >= CURRENT_CACHE_SCHEMA_VERSION;
    let initial_watermark = if has_cached {
        store.hydration_state = HydrationState::CatchUpDelta;
        store.sync_meta.issue_watermark
    } else {
        store.hydration_state = HydrationState::ColdBootstrap;
        None
    };

    let (config_tx, config_rx) = watch::channel(config.clone());
    spawn_notification_buffer_timer(sender.clone(), config_rx.clone());

    let client_cfg = ClientConfig::from_llim_config(&config, offline);
    if !offline && resolve_api_key(&config).is_none() {
        sender.dispatch(LlimEvent::AppError(LlimError::InvalidLinearToken));
    }

    let mut service = LinearService::new(client_cfg, initial_watermark)?;
    if has_cached {
        let unsynced: Vec<Issue> = store
            .issues
            .values()
            .filter(|i| i.details_synced_at != Some(i.updated_at))
            .cloned()
            .collect();
        service.enqueue_issues(&unsynced, None);
    }

    let poller = spawn_linear_poller(service, sender.clone(), config_rx, offline);

    let mut app = LlimApp::new(
        config,
        config_path,
        config_tx,
        store,
        Some(poller),
        sender,
        offline,
    );

    if resolve_api_key(&app.config).is_none() && !offline {
        app.widgets.push_config_popup(app.config.clone());
    }

    Ok(app)
}
