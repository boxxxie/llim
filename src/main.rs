#![recursion_limit = "256"]

pub mod app_init;
pub mod client;
pub mod config;
pub mod dispatcher;
pub mod domain;
pub mod effect_registry;
pub mod event;
pub mod gruvbox;
pub mod id;
pub mod input;
pub mod llim_app;
pub mod logging;
pub mod notice_service;
pub mod notification_buffer;
pub mod os_notify;
pub mod rendering;
pub mod result;
pub mod stores;
pub mod theme;
pub mod tui;
pub mod ui;

use std::{io, path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    app_init::initialize_app,
    client::{config::ClientConfig, service::LinearService},
    config::{
        default_config_path, default_db_path, default_quack_token_path, load_config,
        resolve_api_key,
    },
    domain::HydrationState,
    event::{EventHandler, LlimEvent},
    logging::{LoggingConfig, init_logging},
    rendering::render_main_ui,
    result::Result,
    stores::{LinearStore, resolve_duckdb_binary},
    tui::Tui,
};

#[derive(Debug, Parser)]
#[command(
    name = "llim",
    version,
    about = "Multi-team Linear TUI monitor & client backed by DuckDB 2.0 and the Quack protocol"
)]
struct CliArgs {
    /// Custom path to llim.toml configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Print the default configuration file path and exit
    #[arg(long)]
    print_config_path: bool,

    /// Verify configuration, API key resolution, and DuckDB 2.0 availability and exit
    #[arg(long)]
    verify_config: bool,

    /// Print the DuckDB 2.0 Quack protocol ATTACH snippet for concurrent CLI queries and exit
    #[arg(long)]
    print_quack_attach: bool,

    /// Run a single non-interactive sync into DuckDB 2.0 and exit without launching the TUI
    #[arg(long)]
    sync_once: bool,

    /// Launch in offline mode using cached DuckDB 2.0 rows without polling Linear
    #[arg(long)]
    offline: bool,

    /// Enable verbose debug logging
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();
    let config_path = args.config.unwrap_or_else(default_config_path);

    if args.print_config_path {
        println!("{}", config_path.display());
        return Ok(());
    }

    if args.print_quack_attach {
        let cfg = load_config(&config_path).unwrap_or_default();
        let token_path = default_quack_token_path();
        let token = std::fs::read_to_string(&token_path)
            .unwrap_or_else(|_| "<quack-token>".to_string())
            .trim()
            .to_string();
        println!(
            "LOAD quack;\nCREATE SECRET (TYPE quack, TOKEN '{}');\nATTACH 'quack:127.0.0.1:{}' AS llim;\nSELECT identifier, team_key, state_name, priority, title FROM llim.issues LIMIT 20;",
            token, cfg.database.quack_port
        );
        return Ok(());
    }

    if args.verify_config {
        let cfg = load_config(&config_path)?;
        let key_status = match resolve_api_key(&cfg) {
            Some(src) => {
                let kind = match src {
                    config::SecretSource::EnvVar(_) => "LINEAR_API_KEY env var",
                    config::SecretSource::ConfigFile(_) => "llim.toml [linear].api_key",
                    config::SecretSource::LinearCliFallback(_) => "linear-cli config fallback",
                };
                format!("resolved ({kind})")
            }
            None => "missing (set LINEAR_API_KEY or configure via 'c' in llim)".to_string(),
        };
        let duckdb_bin = resolve_duckdb_binary();
        let db_path = default_db_path();
        let teams = cfg
            .linear
            .teams
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("config_path: {}", config_path.display());
        println!("duckdb_binary: {}", duckdb_bin.display());
        println!("database_path: {}", db_path.display());
        println!(
            "quack_server: {} (127.0.0.1:{})",
            if cfg.database.quack_server_enabled {
                "enabled"
            } else {
                "disabled"
            },
            cfg.database.quack_port
        );
        println!("teams: [{teams}]");
        println!("api_key: {key_status}");
        return Ok(());
    }

    if args.sync_once {
        return run_sync_once(config_path).await;
    }

    let events = EventHandler::new(Duration::from_millis(33));
    let sender = events.sender();
    let _log_guard = init_logging(LoggingConfig::from_env(args.debug), Some(sender.clone()));

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)
        .map_err(|e| result::LlimError::GeneralError(e.to_string().into()))?;
    let mut tui = Tui::new(terminal, events);
    tui.init()?;

    let mut app = initialize_app(config_path, sender, args.offline)?;
    let mut ui_events = Vec::with_capacity(8);

    while app.is_running() {
        let elapsed = app.elapsed_fx_duration();
        tui.draw_frame(|frame| {
            app.terminal_area = frame.area();
            render_main_ui(
                frame,
                &mut app.widgets,
                &app.store,
                &app.rate_limit,
                app.offline,
            );
            let area = frame.area();
            app.effects
                .process_effects(elapsed, frame.buffer_mut(), area);
        })?;

        tui.events.set_fast_tick(app.effects.is_running());

        if let Ok(ev) = tui.events.next() {
            app.apply(ev, &mut ui_events);
            while let Some(next_ev) = tui.events.try_next() {
                app.apply(next_ev, &mut ui_events);
            }
            while !ui_events.is_empty() {
                let batch = std::mem::take(&mut ui_events);
                for derived in batch {
                    app.apply(derived, &mut ui_events);
                }
            }
            if let Some(initial_text) = app.pending_external_editor.take() {
                if let Ok(Some(edited)) = tui.run_external_editor(&initial_text) {
                    app.apply(LlimEvent::ExternalEditorCompleted(edited), &mut ui_events);
                }
            }
        }
    }

    tui.exit()?;
    Ok(())
}

async fn run_sync_once(config_path: PathBuf) -> Result<()> {
    let cfg = load_config(&config_path)?;
    let mut store = LinearStore::new(default_db_path());
    let mut db_cfg = cfg.database.clone();
    db_cfg.quack_server_enabled = false;
    store.init_duckdb_and_load(&db_cfg, None);

    let initial_watermark = if !store.issues.is_empty() && store.sync_meta.issue_watermark.is_some()
    {
        store.hydration_state = HydrationState::CatchUpDelta;
        store.sync_meta.issue_watermark
    } else {
        store.hydration_state = HydrationState::ColdBootstrap;
        None
    };

    let (tx, rx) = mpsc::channel();
    let client_cfg = ClientConfig::from_llim_config(&cfg, false);
    let mut service = LinearService::new(client_cfg, initial_watermark)?;
    service.sync(&tx).await?;

    while let Ok(ev) = rx.try_recv() {
        match ev {
            LlimEvent::InitialHydrationLoaded(payload) => {
                store.apply_bootstrap(*payload, cfg.polling.max_age_days);
            }
            LlimEvent::DeltaBatchLoaded(payload) => {
                store.apply_delta_batch(
                    *payload,
                    cfg.polling.max_activity_events,
                    cfg.polling.max_stored_issues,
                );
            }
            LlimEvent::IssueDetailsLoaded(payload) => {
                store.apply_issue_activity(*payload);
            }
            _ => {}
        }
    }

    store.shutdown();
    // Give background DuckDB writer a brief moment to flush transaction and checkpoint
    tokio::time::sleep(Duration::from_millis(250)).await;

    println!(
        "Synced teams={} projects={} issues={} activity_events={}",
        store.teams.len(),
        store.projects.len(),
        store.issues.len(),
        store.activity_events.len()
    );
    Ok(())
}
