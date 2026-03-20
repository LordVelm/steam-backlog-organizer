# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Steam Backlog Organizer — a Python CLI + GUI tool that categorizes a user's Steam game library into four collections (Completed, In Progress/Backlog, Endless, Not a Game) using rule-based classification. Writes collections directly to Steam's local cloud storage with proper sync metadata so they appear on all machines.

## Commands

```bash
# Setup & run
python -m venv venv
source venv/bin/activate        # or .\venv\Scripts\Activate.ps1 on Windows
pip install requests rich customtkinter

# Run
python organizer.py             # CLI mode
python gui.py                   # GUI mode
python organizer.py --setup     # reconfigure API key / Steam ID
python organizer.py --override  # manually fix game categories

# Build standalone exe
pip install pyinstaller
python build.py                 # outputs to dist/SteamBacklogOrganizer.exe (GUI)
python build.py --cli           # outputs to dist/SteamBacklogOrganizer-CLI.exe
```

No test suite exists.

## Architecture

**organizer.py** is the single-file application with these sections:

- **Config management** — Steam API key and Steam ID in `%APPDATA%/SteamBacklogOrganizer/config/settings.json`. Priority: env vars > saved config > interactive prompt.
- **Steam API integration** — Fetches owned games, playtime, and per-game achievements. Rate-limited at 0.5s between calls. All API calls have timeouts and error handling.
- **Steam collections I/O** — Reads/writes Steam's `cloud-storage-namespace-1.json` in userdata. Also updates `cloud-storage-namespace-1.modified.json` and `cloud-storage-namespaces.json` so Steam syncs changes to the cloud. Requires Steam to be closed.
- **Caching** — Library data cached 24h in `.cache/library.json`. Store API details cached permanently in `.cache/store_details.json`.
- **Saved classifications** — Final results persisted in `.cache/classifications_final.json`. Only new games get classified on subsequent runs.
- **Manual overrides** — User corrections stored in `.config/overrides.json`. Always take priority over rules.
- **Steam Store API** — `fetch_store_details()` gets game type/genres/categories from `store.steampowered.com/api/appdetails`. Rate-limited at 0.3s between calls.
- **Rule-based classification** — `classify_by_rules()` applies 14 priority rules: store type, name patterns, story achievements, high achievement %, multiplayer-only, MMO, genre-based, moderate achievements + playtime, significant SP playtime, unplayed SP, low playtime SP, and fallback. `classify_all_games()` orchestrates: overrides → saved → rules.
- **Output** — Rich tables in terminal, writes Steam collections, optional JSON report.

**gui.py** — CustomTkinter GUI with Simple and Detailed view modes.

**build.py** — PyInstaller wrapper for building a one-file Windows executable.

## Key Design Decisions

- Pure rule-based classification — no external AI API dependency
- Classifications are saved permanently — only new games get classified on subsequent runs
- Manual overrides always win over rules
- Four categories: COMPLETED, IN_PROGRESS, ENDLESS, NOT_A_GAME
- Steam cloud sync requires updating three files, not just the namespace JSON
- Single-file architecture (no package/module structure)
- All API calls have timeouts and proper error handling

## Categories

- **COMPLETED** — User finished the main story/campaign
- **IN_PROGRESS** — Game has a clear ending but user hasn't reached it (includes backlog)
- **ENDLESS** — No real completion state (multiplayer, sandbox, strategy, roguelikes)
- **NOT_A_GAME** — Demos, teasers, tools, utilities, soundtracks, dedicated servers

### 2026 Dashboard Design DNA
- **Theme:** "Steam-meets-Netflix" (Dark Mode).
- **Colors:** Background #171a21, Surface #1b2838, Primary #66c0f4, Accents (Completed: #4CAF50, In Progress: #FFC107, Endless: #2196F3).
- **Layout:** 12-column grid. Use Card-based browsing instead of lists.
- **UI Framework:** CustomTkinter (keep it modern and rounded).
- **Key Fields:** Map `confidence` to visual badges and `reason` to hover tooltips.
