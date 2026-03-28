# Feature plan: HowLongToBeat + “Short games only” filter (Gamekeeper)

**Indexed in:** [`CLAUDE.md`](../CLAUDE.md) (section *Specced feature: HowLongToBeat + “Short games only” filter*) so Claude Code and collaborators see it when reading project guidance.

**Purpose:** This document is a self-contained spec you can paste or upload for an AI (e.g. Claude) to implement the feature. It is **not** an implementation—only the plan.

---

## Product goal

Pull playtime estimates from **HowLongToBeat (HLTB)** and add a **“Short games only”** filter so users can quickly see games that fit the time they have. Many backlog organizers show *what* you own, not *how long* things take; this addresses “Do I have time to start this?”

---

## Codebase context

- **Stack:** Tauri 2 + React. Rust backend in `app/src-tauri/src/`, frontend in `app/src/`.
- **`AppState`** (see `app/src-tauri/src/lib.rs`): holds `games`, `classifications`, `store_cache`, overrides, sync cancel flag.
- **Long operations:** `reqwest` + `tokio::time::sleep`; emit **`sync-progress`** events (pattern in `app/src-tauri/src/steam_api.rs`, struct `SyncProgress`).
- **UI filtering:** Category filter in `app/src/App.tsx` (`filteredGames`). Search/sort in `app/src/components/GameGrid.tsx`. Tauri bindings/types in `app/src/lib/commands.ts`.
- **Persistence:** JSON under `%APPDATA%\Gamekeeper\` via `app/src-tauri/src/config.rs` and `app/src-tauri/src/cache.rs` (cache + config dirs).

---

## Important constraint (HLTB)

HLTB has **no official public API**. Integration must use an **unofficial** approach, for example:

- The [`howlongtobeat`](https://crates.io/crates/howlongtobeat) Rust crate, **or**
- A minimal `reqwest` client that matches HLTB’s **current** search HTTP API (may change without notice).

Treat integration as **best-effort**: aggressive **caching**, user-visible **attribution** (“Data from HowLongToBeat”), and a **Settings** note that unofficial access can break.

---

## Data model

### `HltbEntry` (serde-friendly)

Include at least:

| Field | Role |
|--------|------|
| `main_story_hours: Option<f64>` | Primary metric for “short game” |
| `main_extra_hours: Option<f64>` | Optional; detail UI |
| `completionist_hours: Option<f64>` | Optional; detail UI |
| `hltb_game_id: Option<String>` or `u64` | If available; debugging / future deep links |
| `fetched_at: f64` | Unix timestamp; staleness / refresh later |

### Storage

- **`HashMap<String, HltbEntry>`** keyed by Steam **`appid`** as string (same pattern as `store_cache`).
- File: `hltb_data.json` under cache dir—add `hltb_cache_file()` next to `store_cache_file()` in `config.rs`.

### Do **not** (for v1) embed HLTB inside `Classification` on disk

Keep HLTB in a **parallel cache** so you do not invalidate `classifications_final.json` or `RULES_VERSION` logic in `cache.rs`.

---

## Rust backend

1. **New module** `app/src-tauri/src/hltb.rs` (name flexible):
   - Search HLTB by Steam game name; **v1 matching:** first search result or very simple title heuristic; document false-positive risk.
   - Optionally **skip** fetches for obvious non-games when `store_cache` has `app_type` like DLC/tool/music (reuse `StoreDetails` from `steam_api`).
   - **Rate limiting:** similar delay to store fetch (`STORE_DELAY`-style).
   - **Cancel:** respect `sync_cancelled` like `fetch_store_details_batch`.
   - **Progress:** `emit("sync-progress", …)` with step e.g. `"Fetching HowLongToBeat times"` and `current` / `total`.

2. **`AppState`:** add `hltb_cache: Mutex<HashMap<String, HltbEntry>>`, loaded at startup with other caches in `run()`.

3. **`cache.rs`:** `load_hltb_cache()` / `save_hltb_cache()` mirroring store cache helpers.

4. **Tauri commands** (register in `invoke_handler` in `lib.rs`):
   - **`fetch_hltb_data`** — async; walk library `appid`s; **MVP:** skip appids already present in cache; merge into state; `save_hltb_cache`. (Optional later: `force_refresh`.)
   - **`get_hltb_cache`** — sync; return clone of map for UI after load / after fetch.

5. **Dependency:** add `howlongtobeat` or implement thin HTTP in `Cargo.toml`—verify against live HLTB; if crate is stale, replace with custom `reqwest` + JSON parse isolated in `hltb.rs`.

**Note:** HLTB `Game`-style results may expose `main`, `extra`, `completionist` as **strings** (e.g. `"12½ Hours"`). Parser must convert to `Option<f64>` hours.

---

## Frontend

1. **`commands.ts`:** TypeScript types for `HltbEntry`; `invoke` wrappers for `fetch_hltb_data` and `get_hltb_cache`.

2. **`App.tsx`:**
   - When app reaches **ready**, call `get_hltb_cache()` into React state (e.g. `hltbByAppid`).
   - In **setup complete** and **re-sync** flows: after `fetch_store_details` (and `classify_games` as today), call **`fetch_hltb_data`**. Product choice: gate behind **“Fetch HLTB during sync”** in preferences (default on vs off for faster first run).

3. **“Short games only” filter** (composed **after** category filter, **with** search in grid):
   - **Rule (MVP):** include if `main_story_hours.is_some_and(|h| h <= maxHours)`.
   - **Max hours:** persisted (e.g. `config/preferences.json`) or acceptable alternative; plan recommends file under `config_dir()` for consistency.
   - **Unknown times:** when filter is on, default **exclude** games without `main_story_hours`; add checkbox **“Include games without HLTB data”** (default **off**).
   - **UI:** toggle + max-hours control in `GameGrid.tsx` (toolbar or row under search); update displayed game count.

4. **`GameDetail.tsx`:** If HLTB exists for `appid`, show one line: Main / Main+Extra / Completionist (labels clear).

5. **Settings (`SettingsPanel.tsx`):**
   - Short explanation + link: https://howlongtobeat.com/
   - State estimates are **community-sourced** and integration is **unofficial**.
   - Optional: **“Refresh HLTB data”** / “Fetch on sync” toggle and max-hours default.

---

## End-to-end flow (reference)

1. `fetch_library` → `fetch_store_details` → `classify_games` → **`fetch_hltb_data`**
2. Loop: for each game (with delay), HLTB search → parse → update cache → emit progress
3. Save `hltb_data.json`
4. UI: `get_hltb_cache` → apply **Short games only** when enabled

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| HLTB site/API changes | Isolate HTTP in `hltb.rs`; cache; document fragility |
| Large libraries | Progress + cancel; skip cached appids in MVP |
| Wrong game match | Document; future: pick from top N results |

---

## Testing (manual)

- Small library: filter count matches expectation.
- Cancel mid-HLTB fetch; relaunch: cache loads from disk.
- Optional: unit tests for **pure** helpers only (e.g. parsing hour strings)—**no** network in CI.

---

## Optional phase 2 (out of scope unless requested)

- Add `main_story_hours` to `get_recommendations` candidate JSON for “I only have 2 hours.”
- Deep link from `GameDetail` to HLTB game page (if `hltb_game_id` available).

---

## Suggested implementation checklist (for Claude / implementer)

- [ ] `config.rs`: `hltb_cache_file()`
- [ ] `cache.rs`: load/save HLTB map
- [ ] `hltb.rs`: search, parse times, batch fetch with progress + cancel
- [ ] `lib.rs`: `AppState.hltb_cache`, startup load, `fetch_hltb_data`, `get_hltb_cache`, `mod hltb`
- [ ] `Cargo.toml`: dependency
- [ ] `commands.ts` + `App.tsx` sync + ready paths
- [ ] `GameGrid.tsx`: short filter + threshold + unknown toggle
- [ ] `GameDetail.tsx`: HLTB line
- [ ] `SettingsPanel.tsx` + preferences file: copy, max hours, fetch-on-sync if desired
