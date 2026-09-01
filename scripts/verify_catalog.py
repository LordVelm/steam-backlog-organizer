#!/usr/bin/env python3
"""Verify a catalog.gkc artifact: structural checks + golden-neighbor sanity.

Usage:
    python verify_catalog.py dist/catalog.gkc
    python verify_catalog.py fixtures/catalog_mini.gkc --emit-golden
        (writes fixtures/catalog_mini_neighbors.json for the Rust tests)
"""

from __future__ import annotations

import argparse
import gzip
import json
import struct
import sys
import zlib
from pathlib import Path

import numpy as np

# Windows consoles default to cp1252; game names are full Unicode
sys.stdout.reconfigure(encoding="utf-8", errors="replace")

MAGIC = b"GKC1"
HEADER_SIZE = 48

# anchor appid -> substrings we expect among its top-10 neighbor names
GOLDEN = {
    1145360: ["dead cells", "rogue", "isaac", "slay the spire", "gungeon", "hades"],
    413150: ["portia", "coral", "farm", "valley", "wandering", "harvest", "cozy"],
    1245620: ["souls", "sekiro", "nioh", "lies of p", "hollow", "bloodborne", "ring"],
    730: ["valorant", "call of duty", "pubg", "battlefield", "shooter", "strike", "warface"],
    # Space grand strategy / 4X — full catalog gives space games; the mini
    # fixture's closest defensible matches are its sci-fi/strategy titles
    281990: ["star", "space", "galac", "nova", "4x", "frontier", "stellar",
             "rimworld", "europa", "civilization"],
}


def read_gkc(path: Path):
    data = path.read_bytes()
    body, crc_stored = data[:-4], struct.unpack("<I", data[-4:])[0]
    crc = zlib.crc32(body) & 0xFFFFFFFF
    assert crc == crc_stored, f"CRC mismatch: {crc:#x} != {crc_stored:#x}"

    magic, ver, dim, n, built_at, ds_date, vec_off, meta_off, meta_len = struct.unpack(
        "<4sHHIQIQQQ", body[:HEADER_SIZE]
    )
    assert magic == MAGIC, f"bad magic {magic!r}"
    assert ver == 1, f"unknown format version {ver}"

    appids = np.frombuffer(body, dtype=np.uint32, count=n, offset=HEADER_SIZE)
    scales = np.frombuffer(body, dtype=np.float32, count=n, offset=HEADER_SIZE + 4 * n)
    q = np.frombuffer(body, dtype=np.int8, count=n * dim, offset=vec_off).reshape(n, dim)
    meta = json.loads(gzip.decompress(body[meta_off:meta_off + meta_len]))

    assert np.all(np.diff(appids.astype(np.int64)) > 0), "appids not strictly ascending"
    assert len(meta) == n, f"meta count {len(meta)} != {n}"
    assert all(m["appid"] == int(a) for m, a in zip(meta, appids)), "meta order mismatch"

    vecs = q.astype(np.float32) * scales[:, None]
    norms = np.linalg.norm(vecs, axis=1)
    ok = (norms > 0.90) & (norms < 1.10)
    assert ok.mean() > 0.99, f"only {ok.mean():.1%} of vectors near unit norm"

    print(f"OK  {path.name}: {n} games, dim {dim}, dataset {ds_date}, "
          f"norms mean {norms.mean():.4f}")
    return appids, vecs, meta


def neighbors(appids, vecs, meta, appid: int, k: int = 10):
    idx = np.searchsorted(appids, appid)
    if idx >= len(appids) or appids[idx] != appid:
        return None
    sims = vecs @ vecs[idx]
    order = np.argsort(-sims)
    out = []
    for j in order:
        if int(appids[j]) == appid:
            continue
        out.append((int(appids[j]), meta[j]["name"], float(sims[j])))
        if len(out) == k:
            break
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("path", type=Path)
    ap.add_argument("--emit-golden", action="store_true",
                    help="write fixtures/catalog_mini_neighbors.json (top-5 per anchor)")
    args = ap.parse_args()

    appids, vecs, meta = read_gkc(args.path)

    failures = 0
    golden_out = {}
    for anchor, expected in GOLDEN.items():
        ns = neighbors(appids, vecs, meta, anchor)
        if ns is None:
            print(f"--  anchor {anchor} not in catalog, skipping")
            continue
        names = [n[1] for n in ns]
        joined = " | ".join(names).lower()
        hits = [e for e in expected if e in joined]
        status = "PASS" if hits else "FAIL"
        if not hits:
            failures += 1
        anchor_name = next((m["name"] for m in meta if m["appid"] == anchor), "?")
        print(f"{status}  {anchor_name} ({anchor}) -> {names[:5]}  "
              f"[matched: {hits or 'none'}]")
        golden_out[str(anchor)] = [
            {"appid": a, "name": n} for a, n, _ in ns[:5]
        ]

    if args.emit_golden:
        out = Path(__file__).parent.parent / "fixtures" / "catalog_mini_neighbors.json"
        out.write_text(json.dumps(golden_out, indent=2, ensure_ascii=False),
                       encoding="utf-8")
        print(f"Wrote {out}")

    if failures:
        sys.exit(f"{failures} golden-neighbor checks FAILED")
    print("All golden-neighbor checks passed.")


if __name__ == "__main__":
    main()
