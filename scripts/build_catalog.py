#!/usr/bin/env python3
"""Build Gamekeeper's offline catalog artifact (catalog.gkc) from the
FronkonGames/steam-games-dataset (MIT licensed).

The artifact bundles into the Tauri installer and powers the Taste Engine:
store discovery, taste profiles, anti-recommendations, wishlist scoring.

Usage:
    python build_catalog.py                  # full build -> dist/catalog.gkc
    python build_catalog.py --mini           # also emit fixtures/catalog_mini.gkc
    python build_catalog.py --out DIR        # output directory (default dist/)

Requires: pip install kagglehub ijson model2vec numpy

Data source: the Kaggle copy of FronkonGames/steam-games-dataset (games.json).
The HuggingFace parquet mirror ships an EMPTY tags column (verified 2026-08-31),
and user tags are the taste engine's highest-signal input — do not switch back
to the HF mirror without re-checking tag coverage.
"""

from __future__ import annotations

import argparse
import gzip
import io
import json
import re
import struct
import sys
import time
import zlib
from datetime import datetime, timezone
from pathlib import Path

import numpy as np

MAGIC = b"GKC1"
FORMAT_VERSION = 1
EMBED_DIM = 256
EMBED_MODEL = "minishlab/potion-base-8M"
DATASET_REPO = "FronkonGames/steam-games-dataset"

HEADER_SIZE = 48

ADULT_TAGS = {"sexual content", "hentai", "nsfw", "nudity"}

# Hand-picked, well-known appids for the committed test fixture. Chosen to give
# meaningful golden-neighbor checks across distinct clusters.
MINI_FIXTURE_APPIDS = [
    # Roguelites
    1145360,  # Hades
    588650,   # Dead Cells
    250900,   # The Binding of Isaac: Rebirth
    646570,   # Slay the Spire
    1253920,  # Rogue Legacy 2
    311690,   # Enter the Gungeon
    632360,   # Risk of Rain 2
    # Cozy / farming
    413150,   # Stardew Valley
    1158160,  # Coral Island
    666140,   # My Time at Portia
    1121640,  # The Wandering Village
    # Souls-likes
    570940,   # Dark Souls Remastered
    1245620,  # Elden Ring
    814380,   # Sekiro
    367520,   # Hollow Knight
    # Grand strategy / 4X
    236850,   # Europa Universalis IV
    281990,   # Stellaris
    289070,   # Civilization VI
    # Competitive FPS
    730,      # Counter-Strike 2
    1938090,  # Call of Duty
    578080,   # PUBG
    # Narrative
    1174180,  # Red Dead Redemption 2
    292030,   # The Witcher 3
    1091500,  # Cyberpunk 2077
    # Sim / sandbox
    255710,   # Cities: Skylines
    294100,   # RimWorld
    105600,   # Terraria
    4000,     # Garry's Mod
    # Platformers / indies
    268910,   # Cuphead
    504230,   # Celeste
    391540,   # Undertale
    620,      # Portal 2
]


def log(msg: str) -> None:
    print(f"[build_catalog] {msg}", flush=True)


def download_dataset() -> tuple[Path, "datetime"]:
    """Download the Kaggle copy (games.json has populated user tags)."""
    import kagglehub

    path = Path(kagglehub.dataset_download("fronkongames/steam-games-dataset"))
    games_json = path / "games.json"
    if not games_json.exists():
        sys.exit(f"ERROR: games.json not found in {path}")
    mtime = datetime.fromtimestamp(games_json.stat().st_mtime, tz=timezone.utc)
    log(f"Dataset: {games_json} ({games_json.stat().st_size / 1e6:.0f} MB, "
        f"downloaded {mtime:%Y-%m-%d})")
    return games_json, mtime


def iter_games(games_json: Path):
    """Stream (appid, game_dict) pairs from the ~1 GB games.json without
    loading it all into memory."""
    import ijson

    with open(games_json, "rb") as f:
        yield from ijson.kvitems(f, "")


def coerce_list(value) -> list[str]:
    if value is None:
        return []
    if isinstance(value, (list, tuple)):
        return [str(v).strip() for v in value if str(v).strip()]
    s = str(value).strip()
    if not s:
        return []
    return [t.strip() for t in s.split(",") if t.strip()]


def coerce_tags(value) -> list[str]:
    """games.json tags are {tag: votes}; keep descending-vote order."""
    if isinstance(value, dict):
        ordered = sorted(value.items(), key=lambda kv: -int(kv[1] or 0))
        return [str(k) for k, _ in ordered]
    return coerce_list(value)


def parse_release(value) -> tuple[int, int]:
    """Return (year, month); (0, 0) when unknown."""
    s = "" if value is None else str(value).strip()
    for fmt in ("%b %d, %Y", "%d %b, %Y", "%Y-%m-%d", "%b %Y", "%Y"):
        try:
            dt = datetime.strptime(s, fmt)
            return dt.year, dt.month
        except ValueError:
            continue
    m = re.search(r"(19|20)\d{2}", s)
    return (int(m.group(0)), 0) if m else (0, 0)


def to_int(value, default=0) -> int:
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return default


def build_rows(games_json: Path) -> list[dict]:
    """Stream-filter the raw dataset down to embeddable games. Prints a funnel."""
    funnel = {"total": 0, "no_text": 0, "too_small": 0}
    rows = []
    for appid_str, g in iter_games(games_json):
        funnel["total"] += 1
        appid = to_int(appid_str)
        name = str(g.get("name") or "").strip()
        if not appid or not name:
            continue

        tags = coerce_tags(g.get("tags"))
        genres = coerce_list(g.get("genres"))
        short_desc = str(g.get("short_description") or "").strip()
        if not tags and not short_desc:
            funnel["no_text"] += 1
            continue

        positive = to_int(g.get("positive"))
        negative = to_int(g.get("negative"))
        total_reviews = positive + negative

        owners_raw = str(g.get("estimated_owners") or "0")
        owners_low = 0
        m = re.match(r"([\d,]+)", owners_raw.replace(" ", ""))
        if m:
            owners_low = int(m.group(1).replace(",", ""))

        if total_reviews < 10 and owners_low < 20000:
            funnel["too_small"] += 1
            continue

        required_age = to_int(g.get("required_age"))
        adult = required_age >= 18 or any(t.lower() in ADULT_TAGS for t in tags)

        try:
            price_cents = int(round(float(g.get("price")) * 100))
        except (TypeError, ValueError):
            price_cents = None
        is_free = price_cents == 0

        year, month = parse_release(g.get("release_date"))
        devs = coerce_list(g.get("developers"))[:2]

        pct = int(round(positive / total_reviews * 100)) if total_reviews > 0 else 0

        rows.append(
            {
                "appid": appid,
                "name": name,
                "tags": tags[:10],
                "shortDesc": short_desc[:220],
                "releaseYear": year,
                "releaseMonth": month,
                "isFree": is_free,
                "priceUsdCents": None if is_free else price_cents,
                "reviewTotal": total_reviews,
                "reviewPositivePct": pct,
                "developers": devs,
                "adult": adult,
                # embed inputs (not serialized into metadata)
                "_tags_full": tags[:15],
                "_genres": genres,
                "_desc_full": short_desc[:300],
            }
        )

    # Deduplicate by appid (dataset occasionally repeats)
    seen: set[int] = set()
    deduped = []
    for row in rows:
        if row["appid"] in seen:
            continue
        seen.add(row["appid"])
        deduped.append(row)
    deduped.sort(key=lambda r: r["appid"])

    funnel["kept"] = len(deduped)
    log(f"Filter funnel: {funnel['total']} rows -> {funnel['kept']} games "
        f"(no text: {funnel['no_text']}, too small: {funnel['too_small']})")
    with_tags = sum(1 for r in deduped if r["tags"])
    log(f"Tag coverage: {with_tags}/{len(deduped)} kept games have user tags "
        f"({with_tags / max(len(deduped), 1):.1%})")
    if with_tags / max(len(deduped), 1) < 0.5:
        sys.exit("ERROR: tag coverage below 50% — wrong/stale dataset source?")
    return deduped


def embed_rows(rows: list[dict]) -> np.ndarray:
    from model2vec import StaticModel

    log(f"Loading embedding model {EMBED_MODEL} ...")
    model = StaticModel.from_pretrained(EMBED_MODEL)

    texts = [
        f"{', '.join(r['_tags_full'])}. {', '.join(r['_genres'])}. {r['_desc_full']}"
        for r in rows
    ]
    log(f"Embedding {len(texts)} games ...")
    t0 = time.time()
    vecs = model.encode(texts, show_progress_bar=True)
    vecs = np.asarray(vecs, dtype=np.float32)
    log(f"Embedded in {time.time() - t0:.1f}s, shape {vecs.shape}")
    if vecs.shape[1] != EMBED_DIM:
        sys.exit(f"ERROR: expected {EMBED_DIM} dims, got {vecs.shape[1]}")

    # L2-normalize (zero vectors -> stay zero)
    norms = np.linalg.norm(vecs, axis=1, keepdims=True)
    norms[norms == 0] = 1.0
    return vecs / norms


def quantize_int8(vecs: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Symmetric per-vector int8 quantization. Returns (q, scales)."""
    absmax = np.abs(vecs).max(axis=1)
    absmax[absmax == 0] = 1.0
    scales = (absmax / 127.0).astype(np.float32)
    q = np.clip(np.round(vecs / scales[:, None]), -127, 127).astype(np.int8)
    return q, scales


def write_gkc(path: Path, rows: list[dict], q: np.ndarray, scales: np.ndarray,
              dataset_date: int) -> None:
    n = len(rows)
    appids = np.array([r["appid"] for r in rows], dtype=np.uint32)
    assert np.all(np.diff(appids.astype(np.int64)) > 0), "appids must be strictly ascending"

    meta = [
        {k: v for k, v in r.items() if not k.startswith("_")}
        for r in rows
    ]
    meta_gz = gzip.compress(
        json.dumps(meta, separators=(",", ":"), ensure_ascii=False).encode("utf-8"),
        compresslevel=9,
    )

    vec_off = HEADER_SIZE + 4 * n + 4 * n          # appids + scales
    meta_off = vec_off + n * EMBED_DIM              # int8 vectors

    header = struct.pack(
        "<4sHHIQIQQQ",
        MAGIC,
        FORMAT_VERSION,
        EMBED_DIM,
        n,
        int(time.time()),
        dataset_date,
        vec_off,
        meta_off,
        len(meta_gz),
    )
    assert len(header) == HEADER_SIZE, f"header is {len(header)} bytes"

    buf = io.BytesIO()
    buf.write(header)
    buf.write(appids.tobytes())
    buf.write(scales.tobytes())
    buf.write(q.tobytes())
    buf.write(meta_gz)
    body = buf.getvalue()
    crc = zlib.crc32(body) & 0xFFFFFFFF

    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.write(body)
        f.write(struct.pack("<I", crc))
    log(f"Wrote {path} — {n} games, {path.stat().st_size / 1e6:.1f} MB "
        f"(vectors {n * EMBED_DIM / 1e6:.1f} MB, meta {len(meta_gz) / 1e6:.1f} MB)")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=Path(__file__).parent.parent / "dist")
    ap.add_argument("--mini", action="store_true",
                    help="also emit fixtures/catalog_mini.gkc for Rust tests")
    args = ap.parse_args()

    (games_json, last_modified) = download_dataset()
    dataset_date = int(last_modified.strftime("%Y%m%d")) if last_modified else 0

    rows = build_rows(games_json)
    vecs = embed_rows(rows)
    q, scales = quantize_int8(vecs)

    write_gkc(args.out / "catalog.gkc", rows, q, scales, dataset_date)

    if args.mini:
        idx = {r["appid"]: i for i, r in enumerate(rows)}
        chosen = [a for a in MINI_FIXTURE_APPIDS if a in idx]
        missing = [a for a in MINI_FIXTURE_APPIDS if a not in idx]
        if missing:
            log(f"WARNING: mini fixture missing appids (filtered out?): {missing}")
        order = sorted(chosen)
        mini_rows = [rows[idx[a]] for a in order]
        mini_q = np.stack([q[idx[a]] for a in order])
        mini_scales = np.array([scales[idx[a]] for a in order], dtype=np.float32)
        fixtures = Path(__file__).parent.parent / "fixtures"
        write_gkc(fixtures / "catalog_mini.gkc", mini_rows, mini_q, mini_scales,
                  dataset_date)

    log("Done.")


if __name__ == "__main__":
    main()
