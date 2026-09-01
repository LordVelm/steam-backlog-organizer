use crate::classifier::Classification;
use crate::config;
use crate::hltb::HltbEntry;
use crate::steam_api::{OwnedGame, StoreDetails};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const LIBRARY_CACHE_MAX_AGE_SECS: u64 = 24 * 60 * 60; // 24 hours

/// Bump this whenever classification rules change to force re-classification
/// on next app launch.
const RULES_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct LibraryCache {
    steam_id: String,
    timestamp: f64,
    games: Vec<OwnedGame>,
}

/// Load cached library data if fresh enough.
pub fn load_library_cache(steam_id: &str) -> Option<Vec<OwnedGame>> {
    let path = config::library_cache_file();
    if !path.exists() {
        return None;
    }

    let data = fs::read_to_string(&path).ok()?;
    let cache: LibraryCache = serde_json::from_str(&data).ok()?;

    if cache.steam_id != steam_id {
        return None;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    if (now - cache.timestamp) > LIBRARY_CACHE_MAX_AGE_SECS as f64 {
        return None;
    }

    Some(cache.games)
}

/// Load cached library data regardless of age. For display purposes (playtime,
/// taste profile) where stale data beats no data; never triggers a network sync.
pub fn load_library_cache_any_age(steam_id: &str) -> Option<Vec<OwnedGame>> {
    let path = config::library_cache_file();
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(&path).ok()?;
    let cache: LibraryCache = serde_json::from_str(&data).ok()?;
    if cache.steam_id != steam_id {
        return None;
    }
    Some(cache.games)
}

/// Save library data to cache.
pub fn save_library_cache(steam_id: &str, games: &[OwnedGame]) -> Result<(), String> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Time error: {e}"))?
        .as_secs_f64();

    let cache = LibraryCache {
        steam_id: steam_id.into(),
        timestamp: now,
        games: games.to_vec(),
    };

    let data = serde_json::to_string_pretty(&cache)
        .map_err(|e| format!("Failed to serialize library cache: {e}"))?;
    fs::write(config::library_cache_file(), data)
        .map_err(|e| format!("Failed to write library cache: {e}"))
}

/// Bump when StoreDetails gains fields that existing entries must backfill.
/// v1 = bare map (legacy), v2 = wrapped form with short_description etc.
const STORE_DETAILS_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct StoreCache {
    version: u32,
    details: HashMap<String, StoreDetails>,
}

/// Load store details cache. Reads the v2 wrapped form; falls back to the
/// legacy bare-map form (v1) so existing users keep their cache.
pub fn load_store_cache() -> HashMap<String, StoreDetails> {
    let path = config::store_cache_file();
    if !path.exists() {
        return HashMap::new();
    }
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    parse_store_cache(&data)
}

/// Parse store cache text: v2 wrapped form first, then legacy v1 bare map.
fn parse_store_cache(data: &str) -> HashMap<String, StoreDetails> {
    if let Ok(cache) = serde_json::from_str::<StoreCache>(data) {
        return cache.details;
    }
    serde_json::from_str(data).unwrap_or_default()
}

/// Save store details cache (always writes the v2 wrapped form).
pub fn save_store_cache(cache: &HashMap<String, StoreDetails>) -> Result<(), String> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    let wrapped = StoreCache {
        version: STORE_DETAILS_VERSION,
        details: cache.clone(),
    };
    let data = serde_json::to_string_pretty(&wrapped)
        .map_err(|e| format!("Failed to serialize store cache: {e}"))?;
    fs::write(config::store_cache_file(), data)
        .map_err(|e| format!("Failed to write store cache: {e}"))
}

/// Appids of cached games whose entries predate v2 (no short_description yet)
/// and are worth backfilling for the taste engine.
pub fn store_entries_needing_backfill(cache: &HashMap<String, StoreDetails>) -> Vec<u64> {
    cache
        .iter()
        .filter(|(_, d)| d.short_description.is_none() && d.app_type == "game")
        .filter_map(|(appid, _)| appid.parse::<u64>().ok())
        .collect()
}

#[derive(Serialize, Deserialize)]
struct ClassificationsCache {
    #[serde(default)]
    rules_version: u32,
    classifications: Vec<Classification>,
}

/// Load saved classifications. Returns empty if the rules version doesn't match
/// (forces re-classification with updated rules).
pub fn load_saved_classifications() -> HashMap<u64, Classification> {
    let path = config::classifications_file();
    if !path.exists() {
        return HashMap::new();
    }
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };

    // Try new versioned format first
    if let Ok(cache) = serde_json::from_str::<ClassificationsCache>(&data) {
        if cache.rules_version != RULES_VERSION {
            // Rules changed — discard saved classifications to force re-classification
            return HashMap::new();
        }
        return cache.classifications.into_iter().map(|c| (c.appid, c)).collect();
    }

    // Fall back to old format (plain array) — treat as outdated, force re-classification
    HashMap::new()
}

/// Save classifications to file with the current rules version.
pub fn save_classifications(classifications: &[Classification]) -> Result<(), String> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    let cache = ClassificationsCache {
        rules_version: RULES_VERSION,
        classifications: classifications.to_vec(),
    };
    let data = serde_json::to_string_pretty(&cache)
        .map_err(|e| format!("Failed to serialize classifications: {e}"))?;
    fs::write(config::classifications_file(), data)
        .map_err(|e| format!("Failed to write classifications: {e}"))
}

/// Load HLTB cache.
pub fn load_hltb_cache() -> HashMap<String, HltbEntry> {
    let path = config::hltb_cache_file();
    if !path.exists() {
        return HashMap::new();
    }
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Save HLTB cache.
pub fn save_hltb_cache(cache: &HashMap<String, HltbEntry>) -> Result<(), String> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    let data = serde_json::to_string_pretty(cache)
        .map_err(|e| format!("Failed to serialize HLTB cache: {e}"))?;
    fs::write(config::hltb_cache_file(), data)
        .map_err(|e| format!("Failed to write HLTB cache: {e}"))
}

/// Load user overrides (appid string → category string).
pub fn load_overrides() -> HashMap<String, String> {
    let path = config::overrides_file();
    if !path.exists() {
        return HashMap::new();
    }
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Save user overrides.
pub fn save_overrides(overrides: &HashMap<String, String>) -> Result<(), String> {
    let dir = config::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;
    let data = serde_json::to_string_pretty(overrides)
        .map_err(|e| format!("Failed to serialize overrides: {e}"))?;
    fs::write(config::overrides_file(), data)
        .map_err(|e| format!("Failed to write overrides: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_store_cache_legacy_format() {
        // v1 on-disk shape: bare appid → StoreDetails map, no v2 fields
        let legacy = r#"{
            "440": { "type": "game", "genres": ["Action"], "categories": ["Multi-player"] }
        }"#;
        let parsed = parse_store_cache(legacy);
        assert_eq!(parsed.len(), 1);
        let d = &parsed["440"];
        assert_eq!(d.app_type, "game");
        assert_eq!(d.genres, vec!["Action"]);
        assert!(d.short_description.is_none());
        assert!(d.developers.is_empty());
    }

    #[test]
    fn store_cache_v2_roundtrip() {
        let mut details = HashMap::new();
        details.insert(
            "440".to_string(),
            StoreDetails {
                app_type: "game".into(),
                genres: vec!["Action".into()],
                categories: vec![],
                short_description: Some("Hats.".into()),
                metacritic: Some(92),
                recommendations: Some(1_000_000),
                developers: vec!["Valve".into()],
                release_date: Some("10 Oct 2007".into()),
            },
        );
        let wrapped = StoreCache { version: STORE_DETAILS_VERSION, details };
        let json = serde_json::to_string(&wrapped).unwrap();
        let parsed = parse_store_cache(&json);
        assert_eq!(parsed["440"].short_description.as_deref(), Some("Hats."));
        assert_eq!(parsed["440"].metacritic, Some(92));
    }

    #[test]
    fn backfill_detects_v1_game_entries_only() {
        let mut cache = HashMap::new();
        cache.insert(
            "10".to_string(),
            StoreDetails {
                app_type: "game".into(),
                genres: vec![],
                categories: vec![],
                short_description: None,
                metacritic: None,
                recommendations: None,
                developers: vec![],
                release_date: None,
            },
        );
        cache.insert(
            "20".to_string(),
            StoreDetails {
                app_type: "dlc".into(),
                genres: vec![],
                categories: vec![],
                short_description: None,
                metacritic: None,
                recommendations: None,
                developers: vec![],
                release_date: None,
            },
        );
        cache.insert(
            "30".to_string(),
            StoreDetails {
                app_type: "game".into(),
                genres: vec![],
                categories: vec![],
                short_description: Some("done".into()),
                metacritic: None,
                recommendations: None,
                developers: vec![],
                release_date: None,
            },
        );
        let needs = store_entries_needing_backfill(&cache);
        assert_eq!(needs, vec![10]);
    }
}
