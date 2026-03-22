# Gamekeeper

Your personal Steam library manager.

Automatically categorizes your Steam game library into four collections:

- **Completed** — Games you've finished the main story/campaign
- **In Progress / Backlog** — Completable games you haven't finished yet
- **Endless** — Games with no real ending (multiplayer, sandbox, strategy, etc.)
- **Not a Game** — Demos, tools, utilities, soundtracks, etc.

Results are written directly to your Steam library as collections (synced across machines) and optionally exported as JSON.

## How It Works

Classification uses **rule-based logic** that analyzes your Steam data:

- **Steam Store data** — Game type (demo, tool, DLC), genres, categories (single-player, multiplayer)
- **Achievement data** — Story-completion achievement names, achievement percentage
- **Playtime** — Hours played relative to game type
- **14 priority rules** — Deterministic, auditable, no black boxes

No external AI or paid APIs needed beyond the free Steam Web API key.

## Requirements

- Windows 10/11 (macOS/Linux support planned)
- A [Steam Web API key](https://steamcommunity.com/dev/apikey) (free)
- Your Steam profile's game details set to **Public**
- Optional: ~5 GB disk space for local AI features (auto-downloaded on first use)

## Installation

Download the latest installer from [Releases](https://github.com/LordVelm/steam-backlog-organizer/releases).

Or build from source:

```bash
cd app
npm install
npx tauri build
```

## Features

- **Modern desktop app** — Built with Tauri + React + TypeScript, dark Steam-inspired theme
- **No paid APIs** — Fully rule-based classification using Steam's own data
- **Saved classifications** — Results persist between runs. Only new games get classified
- **Manual overrides** — Fix any game the rules got wrong. Overrides always take priority
- **Cloud sync** — Collections sync across machines via Steam Cloud
- **Caching** — Library and store data cached locally to avoid redundant API calls
- **"What should I play next?"** — Conversational AI chat panel with personalized game recommendations
- **AI ambiguity assistant** — For uncertain classifications, ask AI for a second opinion
- **Vanity URL support** — Enter your Steam ID or custom profile URL name
- **Export** — Download your classifications as JSON

## Optional: Local AI

The app bundles a local AI engine (Qwen2.5-7B) for smarter game recommendations and classification second opinions. On first use, it downloads the model (~3.5 GB) — no external software needed.

To set up: **Settings > AI Assistant > Download AI Model**

The AI is used for:
- **Game recommendations** — "What should I play next?" conversational chat
- **Ambiguity resolution** — Second opinions on uncertain classifications

The AI is **never** used for core classification — rules stay canonical. GPU acceleration is automatic if you have an NVIDIA GPU with CUDA support; CPU fallback is fully functional.

## System Requirements

| | Minimum (without AI) | Recommended (with AI) |
|---|---|---|
| OS | Windows 10 64-bit | Windows 10/11 64-bit |
| RAM | 4 GB | 8 GB (16 GB for best performance) |
| Storage | 100 MB | 5 GB free |
| GPU | Not required | NVIDIA GPU with 4+ GB VRAM (optional) |
| Network | Internet for Steam API sync | Internet for Steam API sync |

## Important Notes

- **First sync takes time** — Steam's API is rate-limited, so fetching achievements and store details for a large library (500+ games) can take 10–15 minutes. This is a one-time process — subsequent launches use cached data and only fetch new games.
- **Steam must be closed** when writing collections
- **API keys are stored locally** in `%APPDATA%/Gamekeeper/config/settings.json`
- **All data stays on your machine** — no cloud services, no telemetry

## Development

Built with Tauri v2 + React 19 + TypeScript + Rust. See [`CLAUDE.md`](./CLAUDE.md) for architecture details.

## License

[MIT](./LICENSE) — Created by [LordVelm](https://github.com/LordVelm)

## Feedback & Support

- **Bug reports & feature requests** — [Open an issue](https://github.com/LordVelm/steam-backlog-organizer/issues)
- **Support the project** — [Buy Me a Coffee](https://buymeacoffee.com/lordvelm)
