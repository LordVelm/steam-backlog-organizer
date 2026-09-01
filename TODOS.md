# TODOS

## Taste Engine

### Catalog delta refresh (Phase 1.5)
**Priority:** P2
Update the bundled catalog between releases: `IStoreService/GetAppList?if_modified_since=` to find changed appids, hydrate via `appdetails` + `appreviews`, publish `taste-pack-<v>.zip` as a `catalog-vN` GitHub release asset (the app-data override path in `catalog_path()` already wins over the bundled copy). Deferred from the v4.0 plan.

### Wishlist name hydration for catalog misses
**Priority:** P2
Unreleased/delisted wishlist items show "App {id}" (capsule image + store link carry identity). Hydrate real names via rate-limited `appdetails` into the store cache. Deferred from v4.0 (noted in pre-landing review).

### Streaming chat responses
**Priority:** P3
llama-server supports SSE (`"stream": true`); the 4B/CPU tiers would feel much better with token streaming in ChatPanel.

## Roadmap

### Phase 2 — Launch push
**Priority:** P1
README rewrite around the Taste Engine, demo GIF of the "I have 2 hours" flow, posts to r/patientgamers, r/Steam, HN. Blocked on: v4.0.0 release build + clean-profile installer UAT.

### Phase 3 — Multi-store (GOG + Epic)
**Priority:** P2
GOG Galaxy local SQLite (`galaxy-2.0.db`) + Epic manifests (`%ProgramData%/Epic/.../Manifests/*.item`) feeding the existing classifier/taste pipeline.

### Phase 4 — Linux / Steam Deck
**Priority:** P2
Steam paths move to `~/.steam/steam/userdata/`; Tauri Linux build; Deck-friendly UI pass.

## App

### Virtualized game grid
**Priority:** P3
GameGrid renders all 571 cards (pre-existing item from CLAUDE.md Phase 3 checklist). Works, but scrolling could be smoother on huge libraries.

### E2E test harness
**Priority:** P3
lib.rs Tauri command glue and React views are live-verified only (flagged in the v4.0 coverage audit). A tauri-driver/WebdriverIO harness would automate the ship test plan in `~/.gstack/projects/`.

## Completed

*(items move here when shipped, tagged with version + date)*
