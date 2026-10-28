# `llim` — Linear TUI Monitor & DuckDB 2.0 Client

`llim` is a fast, keyboard-driven Terminal User Interface (TUI) for monitoring and triaging [Linear](https://linear.app) issues, projects, and activity across multiple teams, inspired by [`glim`](https://github.com/boxxxie/glim).

Built in Rust with `ratatui`, `crossterm`, `tachyonfx`, and `tokio`, `llim` synchronizes Linear GraphQL deltas into a local **DuckDB 2.0** database (`~/.local/share/llim/llim.duckdb`) and optionally serves it over the **Quack protocol** (`quack:127.0.0.1:9494`) for zero-latency SQL queries from CLI scripts and agents while the TUI is running.

---

## Features

- **Three Switchable Views (`1`, `2`, `3`)**:
  - **`1: Issues`**: High-density sortable table of active issues across configured teams.
  - **`2: My Attention`**: Issues assigned to you, blocking active work, urgent priority, or with unacknowledged `@mentions`.
  - **`3: Activity`**: Live chronological feed of state transitions, priority/assignee changes, and comments with collapsible multi-issue bursts.
- **Split-Pane Details Inspector (`a` / `Tab`)**: Live Markdown description, comment thread, and transition history for the highlighted issue.
- **Full Issue & Comment Workflows**:
  - Change workflow state (`x` / `S`), priority (`!` / `i`), assignee (`m` / `A`), project (`M`), or labels (`l`)—including bulk multi-select (`Space` / `v` / `V`).
  - Create (`I` / `Ctrl+n`), edit (`e`), or comment (`c`) with `$EDITOR` integration and local draft persistence.
- **Time-Windowed Notification Buffer**: Deduplicates rapid edits on the same issue, coalesces multi-issue bursts into a digest, and immediately alerts on direct `@mentions` or urgent escalations via `notify-send`.
- **DuckDB 2.0 + Quack Server**: Instant offline-capable startup from local storage and concurrent SQL inspection via `llim --print-quack-attach`.

---

## Installation

```bash
cargo install --path .
```

Ensure `duckdb` (v1.2+ / 2.0 with `quack` extension support) is installed and available on your `PATH` (or `~/.local/bin/duckdb`).

---

## Configuration (`~/.config/llim.toml`)

`llim` resolves your Linear Personal API Key in priority order:
1. `LINEAR_API_KEY` environment variable
2. `linear.api_key` in `~/.config/llim.toml`
3. Fallback to `~/.linear-cli/config.toml` or `~/.config/linear/config.toml`

Example `~/.config/llim.toml`:

```toml
[linear]
teams = ["ENG", "OPS"]

[views]
default_view = "issues"
activity_feed_lines = 8
show_completed_issues = false

[polling]
delta_poll_seconds = 30
full_reconcile_minutes = 20

[database]
quack_server_enabled = true
quack_port = 9494

[notifications]
notification_check_seconds = 10
direct_attention_alert = true
per_issue_dedup = true
coalesce_threshold = 3
desktop_notifications_enabled = true
```

---

## CLI Usage

```bash
# Launch interactive TUI
llim

# Verify configuration, API key resolution, and DuckDB path
llim --verify-config

# Run a single non-interactive delta sync into DuckDB and exit
llim --sync-once

# Print DuckDB Quack ATTACH snippet for concurrent SQL queries
llim --print-quack-attach

# Launch in offline mode using cached DuckDB rows
llim --offline
```

## License

MIT
