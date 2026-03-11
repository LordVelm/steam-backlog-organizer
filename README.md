# Steam Library Organizer

Automatically categorizes your Steam game library using AI into three collections:

- **Completed** - Games you've finished the main story/campaign
- **In Progress / Backlog** - Completable games you haven't finished yet
- **Endless** - Games with no real ending (multiplayer, sandbox, strategy, etc.)

Results are written directly to your Steam library as collections and optionally saved as a JSON file.

## Requirements

- Python 3.12+
- A [Steam Web API key](https://steamcommunity.com/dev/apikey)
- An [Anthropic API key](https://console.anthropic.com/settings/keys) (for Claude AI classification)
- Your Steam profile's game details must be set to **Public**

## Setup

```powershell
# Clone or download this project, then:
cd steam-library-organizer
python -m venv venv

# Activate the virtual environment
# Windows PowerShell:
.\venv\Scripts\Activate.ps1
# Linux/Mac:
source venv/bin/activate

pip install requests anthropic rich
```

## Usage

```powershell
# First run — will prompt for API keys and Steam ID (saved for future runs)
python organizer.py

# Update your saved API keys or Steam ID
python organizer.py --setup
```

On first run, the tool will:

1. Prompt for your Steam API key, Anthropic API key, and Steam ID (saved locally)
2. Fetch your game library and achievement data from Steam
3. Classify each game using Claude AI
4. Display results in the terminal
5. Write `AI: Completed`, `AI: In Progress`, and `AI: Endless` collections to Steam
6. Optionally save a JSON report to your Documents folder

## How It Works

- **Achievement data** is fetched for all played games and used to detect story completion
- **Existing Steam collections** you've created are used as hints (e.g., if you already have a "Completed" collection, those games are respected)
- **AI classification** uses game names, playtime, achievement names/percentages, and your existing collections to make smart categorizations
- Only `AI:` prefixed collections are created/managed — your existing collections are never modified

## Caching

- Library and achievement data is cached locally in `.cache/` to avoid re-fetching from Steam
- Classification progress is saved after each batch, so interrupted runs can resume
- API keys and Steam ID are saved in `.config/` so you only enter them once

## Building a Standalone Executable

```powershell
pip install pyinstaller
python build.py
```

Creates `dist/SteamLibraryOrganizer.exe` — a single file that runs without Python installed.

## Important Notes

- **Steam must be closed** when writing collections — the tool will check and prompt you
- **Steam API rate limits** apply — the tool adds delays between requests to stay under the limit
- **API keys are stored locally** in `.config/settings.json` — never commit this file
