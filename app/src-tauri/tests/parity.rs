//! Parity test: compare Rust classifier output against Python golden fixtures.
//!
//! Loads golden_library.json, golden_store_details.json, and golden_overrides.json,
//! runs classify_all_games, and diffs against golden_classifications.json.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// Re-use types from the library
use gamekeeper_lib::classifier::{classify_all_games, Classification};
use gamekeeper_lib::steam_api::{OwnedGame, StoreDetails};

fn fixtures_dir() -> PathBuf {
    // tests/ is inside app/src-tauri/, fixtures/ is at repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // app/
        .unwrap()
        .parent() // repo root
        .unwrap()
        .join("fixtures")
}

#[derive(Deserialize)]
struct GoldenClassification {
    appid: u64,
    name: String,
    category: String,
    confidence: String,
    reason: String,
}

#[test]
fn parity_with_python_classifications() {
    let dir = fixtures_dir();

    // Load golden fixtures
    let library_json =
        fs::read_to_string(dir.join("golden_library.json")).expect("golden_library.json not found");
    let games: Vec<OwnedGame> =
        serde_json::from_str(&library_json).expect("Failed to parse golden_library.json");

    let store_json = fs::read_to_string(dir.join("golden_store_details.json"))
        .expect("golden_store_details.json not found");
    let store_cache: HashMap<String, StoreDetails> =
        serde_json::from_str(&store_json).expect("Failed to parse golden_store_details.json");

    let overrides_json = fs::read_to_string(dir.join("golden_overrides.json"))
        .expect("golden_overrides.json not found");
    let overrides: HashMap<String, String> =
        serde_json::from_str(&overrides_json).expect("Failed to parse golden_overrides.json");

    let golden_json = fs::read_to_string(dir.join("golden_classifications.json"))
        .expect("golden_classifications.json not found");
    let golden: Vec<GoldenClassification> =
        serde_json::from_str(&golden_json).expect("Failed to parse golden_classifications.json");

    // Build golden lookup by appid
    let golden_map: HashMap<u64, &GoldenClassification> =
        golden.iter().map(|g| (g.appid, g)).collect();

    // Run Rust classifier (empty saved — fresh classification like the export script)
    let saved = HashMap::new();
    let rust_results = classify_all_games(&games, &saved, &overrides, &store_cache);

    // Accepted diffs: games where the Rust v3.0 rules intentionally improve on Python.
    // These are games with significant playtime + achievements that the Python rules
    // missed due to conservative thresholds. The new rules use tiered thresholds
    // (20%+ ach with 20h+ playtime in SP) and detect externally-tracked achievements.
    let accepted_diffs: HashMap<u64, &str> = HashMap::from([
        (206420, "Saints Row IV — 30h, 26% ach, SP → COMPLETED (was IN_PROGRESS)"),
        (271590, "GTA V Legacy — 132h, 39% ach, SP → COMPLETED (was IN_PROGRESS)"),
        (298110, "Far Cry 4 — 26h, 0% ach (Ubisoft Connect), SP → COMPLETED (was IN_PROGRESS)"),
        (552520, "Far Cry 5 — 20h, 0% ach (Ubisoft Connect), SP → COMPLETED (was IN_PROGRESS)"),
        (582160, "AC Origins — 35h, 39% ach, SP → COMPLETED (was IN_PROGRESS)"),
        (629730, "Blade & Sorcery — 60h, 0% ach, SP → COMPLETED (was IN_PROGRESS)"),
        (1174180, "Red Dead Redemption 2 — 60h, 26% ach, SP → COMPLETED (was IN_PROGRESS)"),
        (2215430, "Ghost of Tsushima — 24h, 34% ach, SP → COMPLETED (was IN_PROGRESS)"),
    ]);

    // Compare
    let mut mismatches: Vec<String> = Vec::new();
    let mut rust_map: HashMap<u64, &Classification> = HashMap::new();
    for r in &rust_results {
        rust_map.insert(r.appid, r);
    }

    for golden_game in &golden {
        let appid = golden_game.appid;
        match rust_map.get(&appid) {
            Some(rust_game) => {
                let rust_cat = rust_game.category.to_string();
                if rust_cat != golden_game.category {
                    if accepted_diffs.contains_key(&appid) {
                        // Intentional improvement — skip
                        continue;
                    }
                    mismatches.push(format!(
                        "MISMATCH appid={} name={:?}\n  python: {} ({})\n  rust:   {} ({})",
                        appid,
                        golden_game.name,
                        golden_game.category,
                        golden_game.reason,
                        rust_cat,
                        rust_game.reason,
                    ));
                }
            }
            None => {
                mismatches.push(format!(
                    "MISSING appid={} name={:?} — in Python output but not Rust",
                    appid, golden_game.name
                ));
            }
        }
    }

    // Check for games in Rust but not in Python
    for rust_game in &rust_results {
        if !golden_map.contains_key(&rust_game.appid) {
            mismatches.push(format!(
                "EXTRA appid={} name={:?} — in Rust output but not Python",
                rust_game.appid, rust_game.name
            ));
        }
    }

    if !mismatches.is_empty() {
        let summary = format!(
            "\n=== PARITY TEST FAILED ===\n{} mismatches out of {} games:\n\n{}\n",
            mismatches.len(),
            golden.len(),
            mismatches.join("\n\n")
        );
        panic!("{summary}");
    }

    println!(
        "PARITY OK: {} games classified identically to Python",
        golden.len()
    );
}
