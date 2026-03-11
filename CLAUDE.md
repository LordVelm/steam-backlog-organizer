# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Steam Library Organizer — a Python CLI tool that categorizes a user's Steam game library into three AI-powered collections (Completed, In Progress/Backlog, Endless) using Claude AI, then writes them directly to Steam's local cloud storage as "AI:" prefixed collections.

## Commands

```bash
# Setup & run
python -m venv venv
source venv/bin/activate        # or .\venv\Scripts\Activate.ps1 on Windows
pip install requests anthropic rich

# Run (first run triggers interactive setup for API keys)
python organizer.py
python organizer.py --setup     # reconfigure API keys / Steam ID

# Build standalone exe
pip install pyinstaller
python build.py                 # outputs to dist/SteamLibraryOrganizer.exe
```

No test suite exists.

## Architecture

**organizer.py** (~950 lines) is a single-file application with these functional sections:

- **Config management** — API keys and Steam ID stored in `.config/settings.json`. Priority: env vars > saved config > interactive prompt.
- **Steam API integration** — Fetches owned games, playtime, and per-game achievements via Steam Web API. Rate-limited at 0.5s between calls.
- **Steam collections I/O** — Reads/writes Steam's `cloud-storage-namespace-1.json` in userdata. Requires Steam to be closed. Manages collection IDs and versioning.
- **Caching** — Library data cached 24h in `.cache/library.json`. Classification progress saved per-batch in `.cache/classification_progress.json` for resume on interruption.
- **AI classification** — Sends 25-game batches to Claude (Sonnet) with a system prompt that prioritizes: existing user collections > achievement names > playtime > genre knowledge. Expects JSON responses.
- **Output** — Displays results in Rich tables, writes collections to Steam, optionally saves JSON report.

**build.py** — PyInstaller wrapper that builds a one-file Windows executable.

## Key Design Decisions

- Single-file architecture (no package/module structure)
- Steam must be closed before writing collections (cloud storage file is locked)
- Classification is resumable — interrupted runs pick up from last completed batch
- Existing user-created Steam collections are fed to the AI as context for better classification
- Three fixed categories only: Completed, In Progress/Backlog, Endless
