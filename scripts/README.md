# Catalog build scripts

Dev-machine tooling that produces the offline catalog artifact (`catalog.gkc`)
bundled into the Gamekeeper installer. Users never run this.

## Setup (one-time)

```powershell
cd scripts
python -m venv venv
./venv/Scripts/python.exe -m pip install kagglehub ijson model2vec numpy
```

## Rebuild the catalog (per release)

```powershell
./venv/Scripts/python.exe build_catalog.py --mini
./venv/Scripts/python.exe verify_catalog.py ../dist/catalog.gkc
```

- `dist/catalog.gkc` — the full artifact (~30 MB, ~60–80k games). Copy into
  `app/src-tauri/resources/` before `npx tauri build` (see `bundle.resources`
  in `tauri.conf.json`).
- `--mini` refreshes `fixtures/catalog_mini.gkc` (100-ish well-known games) used
  by `cargo test catalog::`. After refreshing it, also run
  `verify_catalog.py ../fixtures/catalog_mini.gkc --emit-golden` to regenerate
  `fixtures/catalog_mini_neighbors.json`, then re-run the Rust tests.

## What it does

1. Downloads the **Kaggle** copy of
   [FronkonGames/steam-games-dataset](https://www.kaggle.com/datasets/fronkongames/steam-games-dataset)
   (MIT) — ~124k Steam games. Uses `games.json`, whose `tags` field is a
   vote-ordered `{tag: votes}` dict. **Do not switch to the HuggingFace parquet
   mirror**: its tags column was completely empty as of 2026-08-31, and user
   tags are the taste engine's highest-signal input (the build aborts if tag
   coverage drops below 50% as a guard).
2. Filters to embeddable games (`type == game`, has tags/description,
   ≥10 reviews or ≥20k estimated owners). Adult titles are kept but flagged;
   the app filters them by default.
3. Embeds `tags + genres + short_description` per game with
   [potion-base-8M](https://huggingface.co/minishlab/potion-base-8M)
   (256-dim static embeddings, same model the app uses at runtime via
   model2vec-rs — **the spaces must match**, so never change one side alone).
4. L2-normalizes, int8-quantizes (per-vector scale), writes `catalog.gkc`
   (binary format spec: header + appids + scales + vectors + gzipped JSON
   metadata + crc32 — see `write_gkc` in `build_catalog.py`, mirrored by
   `app/src-tauri/src/catalog.rs`).

## Format compatibility

`format_version` in the header is checked by the Rust reader. If you change the
binary layout or the embedding model/dim, bump `FORMAT_VERSION` here **and**
update `catalog.rs` — a version mismatch makes the app fall back to
"catalog unavailable" rather than reading garbage.
