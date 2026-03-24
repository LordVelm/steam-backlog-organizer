use crate::config;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

const HLTB_BASE: &str = "https://howlongtobeat.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// HowLongToBeat completion data for one game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HltbData {
    pub appid: u64,
    /// The name HLTB matched (may differ from Steam name).
    pub hltb_name: String,
    /// Main story hours (None = not available on HLTB).
    pub main_story: Option<f64>,
    /// Main story + extras hours.
    pub main_extra: Option<f64>,
    /// 100% completionist hours.
    pub completionist: Option<f64>,
}

// -- Internal HLTB API types --

#[derive(Debug, Deserialize)]
struct HltbSearchResponse {
    data: Vec<HltbGame>,
}

#[derive(Debug, Deserialize)]
struct HltbGame {
    game_name: String,
    #[serde(default)]
    comp_main: u64, // seconds; 0 = no data
    #[serde(default)]
    comp_plus: u64,
    #[serde(default)]
    comp_100: u64,
}

fn secs_to_hours(secs: u64) -> Option<f64> {
    if secs > 0 {
        Some((secs as f64) / 3600.0)
    } else {
        None
    }
}

// -- Search key discovery & caching --

#[derive(Serialize, Deserialize)]
struct SearchKeyCache {
    key: String,
    timestamp: f64,
}

const KEY_CACHE_TTL_SECS: f64 = 7.0 * 24.0 * 3600.0; // 1 week

fn load_cached_search_key() -> Option<String> {
    let path = config::hltb_key_cache_file();
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(&path).ok()?;
    let cache: SearchKeyCache = serde_json::from_str(&data).ok()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64();

    if (now - cache.timestamp) > KEY_CACHE_TTL_SECS {
        return None;
    }

    Some(cache.key)
}

fn save_search_key(key: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let cache = SearchKeyCache {
        key: key.to_string(),
        timestamp: now,
    };

    if let Ok(data) = serde_json::to_string(&cache) {
        let _ = fs::write(config::hltb_key_cache_file(), data);
    }
}

/// Scrape the HLTB website to discover the current API search key.
/// The key lives inside one of the Next.js static JS chunks.
async fn discover_search_key(client: &Client) -> Result<String, String> {
    let html = client
        .get(HLTB_BASE)
        .header("User-Agent", UA)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .send()
        .await
        .map_err(|e| format!("HLTB homepage fetch failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("HLTB homepage read failed: {e}"))?;

    // Collect script URLs from the page
    let chunk_re = Regex::new(r#"/_next/static/chunks/[^"'\s]+"#).map_err(|e| e.to_string())?;
    let mut script_urls: Vec<String> = Vec::new();
    for cap in chunk_re.find_iter(&html) {
        let url = format!("{HLTB_BASE}{}", cap.as_str());
        if !script_urls.contains(&url) {
            script_urls.push(url);
        }
    }

    // Prefer _app-*.js chunks (most likely to contain the search key)
    script_urls.sort_by_key(|u| if u.contains("_app-") { 0 } else { 1 });

    // Patterns the search key might appear in
    let key_patterns = [
        r#""/api/search/"\.concat\("([a-zA-Z0-9]+)"\)"#,
        r#""/api/search/"\+"([a-zA-Z0-9]+)""#,
        r#"concat\("/api/search/","([a-zA-Z0-9]+)"\)"#,
        r#"fetch\("/api/search/([a-zA-Z0-9]+)"#,
        r#"/api/search/([a-zA-Z0-9]{8,})"#,
    ];

    for url in &script_urls {
        let Ok(resp) = client.get(url).header("User-Agent", UA).send().await else {
            continue;
        };
        let Ok(script) = resp.text().await else {
            continue;
        };

        for pat in &key_patterns {
            let Ok(re) = Regex::new(pat) else { continue };
            if let Some(caps) = re.captures(&script) {
                // Use the last capture group (some patterns have a static prefix group)
                if let Some(m) = caps.iter().skip(1).flatten().last() {
                    let key = m.as_str().to_string();
                    if key.len() >= 6 {
                        return Ok(key);
                    }
                }
            }
        }
    }

    Err("Could not extract HLTB API search key from any script chunk".to_string())
}

/// Return the HLTB API search key, using cache when possible.
async fn get_search_key(client: &Client) -> Result<String, String> {
    if let Some(cached) = load_cached_search_key() {
        return Ok(cached);
    }
    let key = discover_search_key(client).await?;
    save_search_key(&key);
    Ok(key)
}

// -- Game search --

/// Query HLTB for a game by name. Returns the best (first) match or None.
async fn search_game(client: &Client, key: &str, name: &str) -> Option<HltbGame> {
    let search_url = format!("{HLTB_BASE}/api/search/{key}");
    let terms: Vec<&str> = name.split_whitespace().collect();

    let body = serde_json::json!({
        "searchType": "games",
        "searchTerms": terms,
        "searchPage": 1,
        "size": 5,
        "searchOptions": {
            "games": {
                "userId": 0,
                "platform": "",
                "sortCategory": "popular",
                "rangeCategory": "main",
                "rangeTime": {"min": null, "max": null},
                "gameplay": {"perspective": "", "flow": "", "genre": "", "subGenre": ""},
                "rangeYear": {"min": "", "max": ""},
                "modifier": ""
            },
            "users": {"sortCategory": "postcount"},
            "lists": {"sortCategory": "follows"},
            "filter": "",
            "sort": 0,
            "randomizer": 0
        },
        "useCache": true
    });

    let resp = client
        .post(&search_url)
        .header("Content-Type", "application/json")
        .header("User-Agent", UA)
        .header("Referer", HLTB_BASE)
        .header("Origin", HLTB_BASE)
        .json(&body)
        .send()
        .await
        .ok()?;

    let data: HltbSearchResponse = resp.json().await.ok()?;
    data.data.into_iter().next()
}

// -- Cache I/O --

/// Load HLTB data cache from disk (keyed by appid).
pub fn load_hltb_cache() -> HashMap<u64, HltbData> {
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

/// Save HLTB data cache to disk.
pub fn save_hltb_cache(cache: &HashMap<u64, HltbData>) -> Result<(), String> {
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    let data = serde_json::to_string_pretty(cache)
        .map_err(|e| format!("Failed to serialize HLTB cache: {e}"))?;
    fs::write(config::hltb_cache_file(), data)
        .map_err(|e| format!("Failed to write HLTB cache: {e}"))
}

// -- Public fetch API --

/// Fetch HLTB data for all given games, skipping already-cached entries.
///
/// Emits `hltb-progress` Tauri events while running and saves to disk every
/// 10 fetches so progress isn't lost on cancellation.
///
/// Returns the full updated cache (cached + newly fetched).
pub async fn fetch_for_games(
    client: &Client,
    games: &[(u64, String)],
    app: Option<&tauri::AppHandle>,
    cancel: Option<&Arc<AtomicBool>>,
) -> HashMap<u64, HltbData> {
    let mut cache = load_hltb_cache();

    let uncached: Vec<&(u64, String)> = games
        .iter()
        .filter(|(appid, _)| !cache.contains_key(appid))
        .collect();

    if uncached.is_empty() {
        return cache;
    }

    // Discover the API search key before starting the loop
    let key = match get_search_key(client).await {
        Ok(k) => k,
        Err(e) => {
            eprintln!("[HLTB] Key discovery failed: {e}");
            return cache;
        }
    };

    let total = uncached.len();
    let mut fetched = 0usize;
    let mut dirty = 0usize;

    for (appid, name) in &uncached {
        if cancel.map(|c| c.load(Ordering::SeqCst)).unwrap_or(false) {
            break;
        }

        if let Some(handle) = app {
            let _ = handle.emit(
                "hltb-progress",
                serde_json::json!({
                    "step": "Fetching game lengths",
                    "current": fetched,
                    "total": total,
                }),
            );
        }

        let result = search_game(client, &key, name).await;

        let entry = HltbData {
            appid: **appid,
            hltb_name: result
                .as_ref()
                .map(|g| g.game_name.clone())
                .unwrap_or_else(|| name.to_string()),
            main_story: result.as_ref().and_then(|g| secs_to_hours(g.comp_main)),
            main_extra: result.as_ref().and_then(|g| secs_to_hours(g.comp_plus)),
            completionist: result.as_ref().and_then(|g| secs_to_hours(g.comp_100)),
        };

        cache.insert(**appid, entry);
        fetched += 1;
        dirty += 1;

        // Flush to disk every 10 games to preserve progress
        if dirty >= 10 {
            let _ = save_hltb_cache(&cache);
            dirty = 0;
        }

        // Respectful rate limiting
        sleep(Duration::from_millis(650)).await;
    }

    // Final flush
    let _ = save_hltb_cache(&cache);

    cache
}
