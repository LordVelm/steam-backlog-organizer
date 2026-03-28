# Changelog

All notable changes to Gamekeeper will be documented in this file.

## [3.2.0] - 2026-03-28

### Added
- HowLongToBeat integration: completion time estimates for your entire Steam library, fetched automatically after each sync
- "Short games" filter: toggle + adjustable max hours slider to find games that fit your schedule
- Game detail panel shows HLTB completion times (Main Story, Main + Extra, Completionist) with ~Time Left calculation based on your actual playtime
- AI chat is now time-aware: ask "something short" or "I have 2 hours" and it prioritizes games by estimated time remaining
- HLTB data cached locally with incremental background fetch, progress indicator, and ETA
- Filter settings persist across app restarts

### Changed
- AI model upgraded to Qwen3.5-9B Q8_0 (~9.5 GB) for better chat quality and thinking mode support
- Settings panel now includes Completion Times section with HLTB attribution and explanation

### Fixed
- "~Time Left" now shows actual remaining time (HLTB estimate minus your playtime) instead of raw total
- Title normalization only strips edition suffixes at the end of names, preventing false truncation
- Separate cancel flag for HLTB fetch so cancelling Steam sync doesn't permanently break completion time fetching
- Concurrent HLTB fetch guard prevents double requests if you sync twice quickly
- HLTB cache poll only runs while fetch is in progress (was polling every 10s forever)
- Mutex locks in background tasks use graceful error handling instead of panicking
- Cache save errors are logged instead of silently swallowed

## [3.1.0] - 2026-03-27

### Added
- Bundled AI chat: "What should I play next?" powered by local Qwen model
- Ambiguity assistant for medium-confidence classifications
- GPU/CPU auto-detection with CUDA support
- Custom app icon (gamepad + checkmark)
- Dark "Steam-meets-Netflix" theme with animations

### Changed
- Full rewrite from Python to Tauri + React + TypeScript + Rust
- Single binary, no external dependencies required
