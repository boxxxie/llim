# Changelog

## llim 0.1.1 - 2026-11-01

### Added
- **Epic & Sub-Issue Hierarchy:** Added parent/child epic sub-issue tracking, interactive sub-issue navigation inside the issue details popup, and live hierarchy reconciliation across cached issues.
- **Rich Markdown Rendering:** Added structured Markdown rendering (`pulldown-cmark`) for issue descriptions and comments.
- **Mouse Navigation & Clickable Links:** Added mouse hover highlighting and click-to-open support across tables, popups, and URLs.
- **Resilient GraphQL Deserialization & Diagnostics:** Added per-node GraphQL fallback parsing and structured parse error dumps.

### Changed
- **Memory & Thread Optimization (TUI):** Restricted glibc `malloc` to a single arena (`mallopt(M_ARENA_MAX, 1)`) and configured the Tokio runtime with a single worker thread (`worker_threads = 1`), reducing TUI threads from `21` to `6` and virtual memory (`VSZ`) from `1.07 GB` to `47.0 MB` (`~10.6 MB` private heap).
- **DuckDB 2.0 Worker Optimization:** Configured the persistent DuckDB 2.0 Quack worker (`127.0.0.1:9494`) with single-threaded execution (`threads=1`, `external_threads=1`, `async_threads=1`), `MALLOC_ARENA_MAX=1`, an `8MB` buffer pool limit (`allocator_flush_threshold='512KB'`), and `wal_autocheckpoint='256KB'` plus startup `CHECKPOINT;`, reducing worker threads from `81` to `3` and private heap from `91 MB` to `~15.6 MB`.
- **Tail-Bounded Startup Hydration:** Bounded startup `load_existing_rows` queries across all 8 tables with `-readonly` and `ORDER BY rowid DESC LIMIT ...` and set default in-memory caps (`max_stored_issues = 500`, `max_activity_events = 300`) while preserving 100% of historical rows in `~/.local/share/llim/llim.duckdb`.

## llim 0.1.0 - 2026-10-29

### Added
- Initial release of `llim` read-only multi-team Linear TUI monitor backed by DuckDB 2.0 and Quack protocol (`quack:127.0.0.1:9494`).
