//! Reader for the offline catalog artifact (`catalog.gkc`) built by
//! `scripts/build_catalog.py`. Binary layout (little-endian, N games, D dims):
//!
//! ```text
//! HEADER 48B: magic "GKC1" | format_version u16 | embed_dim u16 | game_count u32
//!             | built_at u64 | dataset_date u32 | vec_off u64 | meta_off u64 | meta_len u64
//! SECTION A (off 48):   appids  u32×N sorted ascending
//! SECTION B (48+4N):    scales  f32×N (per-vector dequant scale)
//! SECTION C (vec_off):  vectors i8×N×D row-major; v[j] = q[j] * scale_i
//!                       (unit-norm pre-quantization, so dot ≈ cosine)
//! SECTION D (meta_off): gzip(JSON [CatalogGameMeta])
//! FOOTER 4B: crc32(all prior bytes)
//! ```

use flate2::read::GzDecoder;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub const EMBED_DIM: usize = 256;
const MAGIC: &[u8; 4] = b"GKC1";
const SUPPORTED_FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: usize = 48;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogHeader {
    pub format_version: u16,
    pub embed_dim: u16,
    pub game_count: u32,
    pub built_at: u64,
    /// yyyymmdd of the source dataset snapshot (shown in Settings).
    pub dataset_date: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogGameMeta {
    pub appid: u32,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub short_desc: String,
    #[serde(default)]
    pub release_year: u16,
    #[serde(default)]
    pub release_month: u8,
    #[serde(default)]
    pub is_free: bool,
    #[serde(default)]
    pub price_usd_cents: Option<u32>,
    #[serde(default)]
    pub review_total: u32,
    #[serde(default)]
    pub review_positive_pct: u8,
    #[serde(default)]
    pub developers: Vec<String>,
    #[serde(default)]
    pub adult: bool,
}

#[derive(Debug)]
pub struct Catalog {
    pub header: CatalogHeader,
    appids: Vec<u32>,
    scales: Vec<f32>,
    vectors: Vec<i8>, // game_count * EMBED_DIM, row-major
    pub meta: Vec<CatalogGameMeta>,
    by_appid: HashMap<u32, u32>,
}

fn read_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn read_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

impl Catalog {
    /// Load and fully validate a catalog file. Corrupt or unknown-version files
    /// return Err; callers fall back to the bundled copy or "catalog unavailable".
    pub fn load(path: &Path) -> Result<Catalog, String> {
        let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_bytes(&data)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Catalog, String> {
        if data.len() < HEADER_SIZE + 4 {
            return Err("catalog file too small".into());
        }

        let (body, footer) = data.split_at(data.len() - 4);
        let crc_stored = u32::from_le_bytes(footer.try_into().unwrap());
        let crc_actual = crc32fast::hash(body);
        if crc_stored != crc_actual {
            return Err(format!(
                "catalog crc mismatch: stored {crc_stored:#010x}, actual {crc_actual:#010x}"
            ));
        }

        if &body[0..4] != MAGIC {
            return Err("bad catalog magic".into());
        }
        let format_version = read_u16(body, 4);
        if format_version != SUPPORTED_FORMAT_VERSION {
            return Err(format!("unsupported catalog format version {format_version}"));
        }
        let embed_dim = read_u16(body, 6);
        if embed_dim as usize != EMBED_DIM {
            return Err(format!("unexpected embed dim {embed_dim}"));
        }
        let game_count = read_u32(body, 8) as usize;
        let built_at = read_u64(body, 12);
        let dataset_date = read_u32(body, 20);
        // Header offsets are untrusted input: all arithmetic must be checked
        // (a crafted u64 near MAX would wrap in release builds and pass naive
        // bounds checks, then panic on slicing).
        let vec_off = usize::try_from(read_u64(body, 24)).map_err(|_| "vec_off overflow")?;
        let meta_off = usize::try_from(read_u64(body, 32)).map_err(|_| "meta_off overflow")?;
        let meta_len = usize::try_from(read_u64(body, 40)).map_err(|_| "meta_len overflow")?;

        let appids_end = game_count
            .checked_mul(4)
            .and_then(|n| n.checked_add(HEADER_SIZE))
            .ok_or("catalog game_count overflow")?;
        let scales_end = game_count
            .checked_mul(4)
            .and_then(|n| n.checked_add(appids_end))
            .ok_or("catalog game_count overflow")?;
        let vec_end = game_count
            .checked_mul(EMBED_DIM)
            .and_then(|n| n.checked_add(vec_off))
            .ok_or("catalog vector size overflow")?;
        let meta_end = meta_off
            .checked_add(meta_len)
            .ok_or("catalog meta size overflow")?;
        if scales_end != vec_off || vec_end != meta_off || meta_end > body.len() {
            return Err("catalog section offsets inconsistent".into());
        }

        let appids: Vec<u32> = body[HEADER_SIZE..appids_end]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        if !appids.windows(2).all(|w| w[0] < w[1]) {
            return Err("catalog appids not strictly ascending".into());
        }

        let scales: Vec<f32> = body[appids_end..scales_end]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        if !scales.iter().all(|s| s.is_finite()) {
            return Err("catalog contains non-finite scales".into());
        }

        // i8 reinterpretation of the raw bytes
        let vectors: Vec<i8> = body[vec_off..vec_end].iter().map(|&b| b as i8).collect();

        // Cap decompressed size: gzip can inflate ~1000:1, and the update-path
        // catalog file is user-writable — an unbounded read is an OOM bomb.
        const MAX_META_BYTES: u64 = 256 * 1024 * 1024;
        let mut gz = GzDecoder::new(&body[meta_off..meta_end]).take(MAX_META_BYTES);
        let mut meta_json = String::new();
        gz.read_to_string(&mut meta_json)
            .map_err(|e| format!("catalog metadata gunzip: {e}"))?;
        if meta_json.len() as u64 >= MAX_META_BYTES {
            return Err("catalog metadata exceeds size limit".into());
        }
        let meta: Vec<CatalogGameMeta> = serde_json::from_str(&meta_json)
            .map_err(|e| format!("catalog metadata parse: {e}"))?;
        if meta.len() != game_count {
            return Err(format!(
                "catalog metadata count {} != game count {game_count}",
                meta.len()
            ));
        }
        // Metadata rows MUST align with the appid index — a misordered file
        // would silently attach every name/tag/score (and the adult flag!)
        // to the wrong game.
        for (i, m) in meta.iter().enumerate() {
            if m.appid != appids[i] {
                return Err(format!(
                    "catalog metadata misaligned at row {i}: meta appid {} != index appid {}",
                    m.appid, appids[i]
                ));
            }
        }

        let by_appid: HashMap<u32, u32> = appids
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u32))
            .collect();

        Ok(Catalog {
            header: CatalogHeader {
                format_version,
                embed_dim,
                game_count: game_count as u32,
                built_at,
                dataset_date,
            },
            appids,
            scales,
            vectors,
            meta,
            by_appid,
        })
    }

    pub fn len(&self) -> usize {
        self.appids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.appids.is_empty()
    }

    /// Row index + metadata for an appid, if present.
    pub fn get(&self, appid: u32) -> Option<(u32, &CatalogGameMeta)> {
        let row = *self.by_appid.get(&appid)?;
        Some((row, &self.meta[row as usize]))
    }

    /// Dequantized (≈unit-norm) vector for a row.
    pub fn vector_f32(&self, row: u32) -> [f32; EMBED_DIM] {
        let row = row as usize;
        let scale = self.scales[row];
        let start = row * EMBED_DIM;
        let mut out = [0.0f32; EMBED_DIM];
        for (o, &q) in out.iter_mut().zip(&self.vectors[start..start + EMBED_DIM]) {
            *o = q as f32 * scale;
        }
        out
    }

    /// Dot product of an f32 query against a stored int8 row (dequantized on the fly).
    fn dot_row(&self, query: &[f32; EMBED_DIM], row: usize) -> f32 {
        let start = row * EMBED_DIM;
        let mut acc = 0.0f32;
        for (q, &v) in query.iter().zip(&self.vectors[start..start + EMBED_DIM]) {
            acc += q * v as f32;
        }
        acc * self.scales[row]
    }

    /// Brute-force scored scan over the whole catalog. `filter` runs inline
    /// before scoring; returns the top-k (row, similarity) pairs, best first.
    pub fn top_matches<F>(&self, query: &[f32; EMBED_DIM], k: usize, filter: F) -> Vec<(u32, f32)>
    where
        F: Fn(u32, &CatalogGameMeta) -> bool + Sync,
    {
        let n = self.len();
        let chunk = 4096;
        let mut scored: Vec<(u32, f32)> = (0..n)
            .into_par_iter()
            .chunks(chunk)
            .map(|rows| {
                let mut local: Vec<(u32, f32)> = Vec::new();
                for row in rows {
                    let r32 = row as u32;
                    if !filter(r32, &self.meta[row]) {
                        continue;
                    }
                    local.push((r32, self.dot_row(query, row)));
                }
                // keep each chunk's top-k to bound the merge
                local.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
                local.truncate(k);
                local
            })
            .reduce(Vec::new, |mut a, b| {
                a.extend(b);
                a
            });
        scored.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name)
    }

    fn load_mini() -> Catalog {
        Catalog::load(&fixture_path("catalog_mini.gkc")).expect("mini fixture must load")
    }

    #[test]
    fn load_mini_fixture() {
        // Python-writer / Rust-reader parity: structure, counts, spot values
        let cat = load_mini();
        assert_eq!(cat.header.format_version, 1);
        assert_eq!(cat.header.embed_dim as usize, EMBED_DIM);
        assert!(cat.len() >= 25, "expected ~30 games, got {}", cat.len());
        assert_eq!(cat.header.game_count as usize, cat.len());
        assert!(cat.header.dataset_date >= 20250101);

        let (_, stardew) = cat.get(413150).expect("Stardew Valley present");
        assert_eq!(stardew.name, "Stardew Valley");
        assert!(!stardew.tags.is_empty());
        assert!(stardew.review_total > 0);
        assert!(!stardew.adult);
    }

    #[test]
    fn vectors_are_near_unit_norm() {
        let cat = load_mini();
        for row in 0..cat.len() as u32 {
            let v = cat.vector_f32(row);
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (0.9..1.1).contains(&norm),
                "row {row} norm {norm} not near 1.0"
            );
        }
    }

    #[test]
    fn corrupt_crc_rejected() {
        let path = fixture_path("catalog_mini.gkc");
        let mut data = std::fs::read(path).unwrap();
        let mid = data.len() / 2;
        data[mid] ^= 0xFF;
        let err = Catalog::from_bytes(&data).unwrap_err();
        assert!(err.contains("crc"), "expected crc error, got: {err}");
    }

    #[test]
    fn truncated_file_rejected() {
        let path = fixture_path("catalog_mini.gkc");
        let data = std::fs::read(path).unwrap();
        let truncated = &data[..data.len() / 2];
        assert!(Catalog::from_bytes(truncated).is_err());
    }

    #[test]
    fn unknown_version_rejected() {
        let path = fixture_path("catalog_mini.gkc");
        let mut data = std::fs::read(path).unwrap();
        data[4] = 99; // bump format_version
        // fix crc so we hit the version check, not the crc check
        let body_len = data.len() - 4;
        let crc = crc32fast::hash(&data[..body_len]);
        data[body_len..].copy_from_slice(&crc.to_le_bytes());
        let err = Catalog::from_bytes(&data).unwrap_err();
        assert!(err.contains("version"), "expected version error, got: {err}");
    }

    #[test]
    fn golden_neighbors() {
        // Expected top-5 neighbors emitted by scripts/verify_catalog.py from the
        // same fixture — guards Python/Rust scoring parity and embedding drift.
        let cat = load_mini();
        let golden: HashMap<String, Vec<serde_json::Value>> = serde_json::from_str(
            &std::fs::read_to_string(fixture_path("catalog_mini_neighbors.json")).unwrap(),
        )
        .unwrap();

        for (anchor_str, expected) in &golden {
            let anchor: u32 = anchor_str.parse().unwrap();
            let (row, _) = cat.get(anchor).expect("anchor in fixture");
            let query = cat.vector_f32(row);
            let got = cat.top_matches(&query, 6, |r, _| r != row);
            let got_ids: Vec<u32> = got.iter().take(5).map(|(r, _)| {
                cat.meta[*r as usize].appid
            }).collect();
            let expected_ids: Vec<u32> = expected
                .iter()
                .map(|v| v["appid"].as_u64().unwrap() as u32)
                .collect();
            // Same set in the same order (scores are deterministic)
            assert_eq!(
                got_ids, expected_ids,
                "neighbor mismatch for anchor {anchor} ({})",
                cat.meta[row as usize].name
            );
        }
    }

    #[test]
    fn filter_predicate_applies() {
        let cat = load_mini();
        let (row, _) = cat.get(1145360).expect("Hades in fixture");
        let query = cat.vector_f32(row);
        // Filter out everything but free games — every result must satisfy it
        let only_high_review = cat.top_matches(&query, 10, |_, m| m.review_total > 100_000);
        for (r, _) in &only_high_review {
            assert!(cat.meta[*r as usize].review_total > 100_000);
        }
    }
}
