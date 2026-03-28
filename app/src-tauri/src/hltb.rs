use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::Emitter;
use tokio::time::sleep;

use crate::cache;
use crate::steam_api::SyncProgress;

const HLTB_BASE: &str = "https://howlongtobeat.com";
const HLTB_API_PATH: &str = "api/find";
const HLTB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const REQUEST_DELAY: Duration = Duration::from_millis(333); // ~3 req/sec
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HltbEntry {
    pub main_story_hours: Option<f64>,
    pub main_extra_hours: Option<f64>,
    pub completionist_hours: Option<f64>,
    pub hltb_game_id: Option<String>,
    pub match_status: String, // "matched" or "no_match"
    pub fetched_at: f64,
}

/// Normalize a game title for HLTB matching.
/// Lowercase, strip trademark/copyright symbols, remove edition suffixes.
pub fn normalize_title(title: &str) -> String {
    let mut s = title.to_lowercase();
    // Strip trademark/copyright symbols
    for ch in &['™', '®', '©'] {
        s = s.replace(*ch, "");
    }
    // Remove common edition suffixes
    let suffixes = [
        " game of the year edition",
        " goty edition",
        " special edition",
        " definitive edition",
        " complete edition",
        " deluxe edition",
        " ultimate edition",
        " enhanced edition",
        " remastered",
        " hd",
        " - digital edition",
    ];
    for suffix in &suffixes {
        if s.ends_with(suffix) {
            s.truncate(s.len() - suffix.len());
        }
    }
    // Normalize whitespace
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compute similarity ratio between two strings (simple Levenshtein-based).
fn similarity(a: &str, b: &str) -> f64 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    if a == b {
        return 1.0;
    }
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein(&a, &b);
    1.0 - (distance as f64 / max_len as f64)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Convert HLTB seconds to hours, rounded to 1 decimal.
fn seconds_to_hours(seconds: i64) -> Option<f64> {
    if seconds <= 0 {
        return None;
    }
    Some((seconds as f64 / 3600.0 * 10.0).round() / 10.0)
}

struct HltbAuth {
    token: String,
    hp_key: String,
    hp_val: String,
}

struct HltbClient {
    client: reqwest::Client,
    search_url: String,
    auth: Option<HltbAuth>,
}

impl HltbClient {
    async fn new() -> Result<Self, String> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(HLTB_UA));
        headers.insert(REFERER, HeaderValue::from_str(&format!("{HLTB_BASE}/")).unwrap());

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("Failed to create HLTB client: {e}"))?;

        let search_url = format!("{HLTB_BASE}/{HLTB_API_PATH}/");
        let auth = Self::fetch_auth(&client).await;

        eprintln!("[HLTB] Search URL: {}", search_url);
        eprintln!("[HLTB] Auth: {}", if auth.is_some() { "ok" } else { "failed" });

        Ok(Self {
            client,
            search_url,
            auth,
        })
    }

    async fn fetch_auth(client: &reqwest::Client) -> Option<HltbAuth> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let init_url = format!("{HLTB_BASE}/{HLTB_API_PATH}/init?t={now_ms}");

        let resp = client.get(&init_url).send().await.ok()?;
        let json: serde_json::Value = resp.json().await.ok()?;

        let token = json.get("token")?.as_str()?.to_string();
        let hp_key = json.get("hpKey")?.as_str()?.to_string();
        let hp_val = json.get("hpVal")?.as_str()?.to_string();

        Some(HltbAuth { token, hp_key, hp_val })
    }

    async fn refresh_auth(&mut self) {
        self.auth = Self::fetch_auth(&self.client).await;
    }

    async fn search(&mut self, title: &str) -> Result<Option<HltbSearchResult>, HltbError> {
        let search_terms: Vec<&str> = title.split_whitespace().collect();

        let auth = match &self.auth {
            Some(a) => a,
            None => {
                self.refresh_auth().await;
                match &self.auth {
                    Some(a) => a,
                    None => return Err(HltbError::Network("No auth token".to_string())),
                }
            }
        };

        // Build body with honeypot field injected
        let mut body = serde_json::json!({
            "searchType": "games",
            "searchTerms": search_terms,
            "searchPage": 1,
            "size": 5,
            "searchOptions": {
                "games": {
                    "userId": 0,
                    "platform": "",
                    "sortCategory": "popular",
                    "rangeCategory": "main",
                    "rangeTime": { "min": 0, "max": 0 },
                    "gameplay": {
                        "perspective": "",
                        "flow": "",
                        "genre": "",
                        "difficulty": ""
                    },
                    "rangeYear": { "max": "", "min": "" },
                    "modifier": ""
                },
                "users": { "sortCategory": "postcount" },
                "lists": { "sortCategory": "follows" },
                "filter": "",
                "sort": 0,
                "randomizer": 0
            },
            "useCache": true
        });

        // Inject honeypot key/value into body
        body[&auth.hp_key] = serde_json::Value::String(auth.hp_val.clone());

        let response = self.client
            .post(&self.search_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "*/*")
            .header("origin", HLTB_BASE)
            .header("x-auth-token", &auth.token)
            .header("x-hp-key", &auth.hp_key)
            .header("x-hp-val", &auth.hp_val)
            .json(&body)
            .send()
            .await
            .map_err(|e| HltbError::Network(e.to_string()))?;

        let status = response.status();

        // On 403 (expired token), refresh auth and signal retry
        if status == 403 {
            eprintln!("[HLTB] Auth expired, refreshing...");
            return Err(HltbError::AuthExpired);
        }

        if status == 429 {
            return Err(HltbError::RateLimited);
        }

        if !status.is_success() {
            eprintln!("[HLTB] HTTP error: {status}");
            return Err(HltbError::Network(format!("HTTP {}", status)));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| HltbError::Parse(e.to_string()))?;

        let results = data.get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();

        if results.is_empty() {
            return Ok(None);
        }

        let first = &results[0];
        Ok(Some(HltbSearchResult {
            game_id: first.get("game_id").and_then(|v| v.as_u64()).unwrap_or(0),
            game_name: first.get("game_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            comp_main: first.get("comp_main").and_then(|v| v.as_i64()).unwrap_or(0),
            comp_plus: first.get("comp_plus").and_then(|v| v.as_i64()).unwrap_or(0),
            comp_100: first.get("comp_100").and_then(|v| v.as_i64()).unwrap_or(0),
        }))
    }
}

#[derive(Debug)]
#[allow(dead_code)]
enum HltbError {
    Network(String),
    RateLimited,
    AuthExpired,
    Parse(String),
}

struct HltbSearchResult {
    game_id: u64,
    game_name: String,
    comp_main: i64,
    comp_plus: i64,
    comp_100: i64,
}

/// Search HLTB for a single game and return an HltbEntry if matched.
async fn search_game(
    hltb: &mut HltbClient,
    steam_title: &str,
) -> Result<HltbEntry, HltbError> {
    let normalized = normalize_title(steam_title);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let result = hltb.search(&normalized).await?;

    match result {
        Some(r) => {
            // Confidence check: compare HLTB title against Steam title
            let hltb_normalized = normalize_title(&r.game_name);
            let sim = similarity(&normalized, &hltb_normalized);

            if sim < 0.7 {
                // Low confidence match, treat as no match
                return Ok(HltbEntry {
                    main_story_hours: None,
                    main_extra_hours: None,
                    completionist_hours: None,
                    hltb_game_id: None,
                    match_status: "no_match".to_string(),
                    fetched_at: now,
                });
            }

            Ok(HltbEntry {
                main_story_hours: seconds_to_hours(r.comp_main),
                main_extra_hours: seconds_to_hours(r.comp_plus),
                completionist_hours: seconds_to_hours(r.comp_100),
                hltb_game_id: Some(r.game_id.to_string()),
                match_status: "matched".to_string(),
                fetched_at: now,
            })
        }
        None => Ok(HltbEntry {
            main_story_hours: None,
            main_extra_hours: None,
            completionist_hours: None,
            hltb_game_id: None,
            match_status: "no_match".to_string(),
            fetched_at: now,
        }),
    }
}

/// Batch fetch HLTB data for a list of games.
/// Priority order: In Progress first, then Endless, then rest.
/// Writes cache to disk after each successful fetch.
/// Emits sync-progress events.
pub async fn fetch_hltb_batch(
    games: Vec<(u64, String, String)>, // (appid, title, category)
    existing_cache: HashMap<String, HltbEntry>,
    app: tauri::AppHandle,
    cancel: std::sync::Arc<AtomicBool>,
) -> Result<HashMap<String, HltbEntry>, String> {
    let mut hltb = HltbClient::new().await?;
    let mut cache = existing_cache;

    // Sort by priority: IN_PROGRESS first, ENDLESS second, rest last
    let mut sorted_games = games;
    sorted_games.sort_by_key(|(_, _, cat)| match cat.as_str() {
        "IN_PROGRESS" => 0,
        "ENDLESS" => 1,
        _ => 2,
    });

    // Filter to only uncached games (skip "matched" and "no_match", retry absent/failed)
    let to_fetch: Vec<(u64, String)> = sorted_games
        .iter()
        .filter(|(appid, _, _)| !cache.contains_key(&appid.to_string()))
        .map(|(appid, title, _)| (*appid, title.clone()))
        .collect();

    let total = to_fetch.len();
    eprintln!("[HLTB] Games to fetch: {}, already cached: {}", total, cache.len());
    if total == 0 {
        return Ok(cache);
    }

    let mut backoff = BACKOFF_INITIAL;
    let mut matched_count: usize = 0;

    for (i, (appid, title)) in to_fetch.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }

        let _ = app.emit("sync-progress", SyncProgress {
            step: "Fetching completion times".into(),
            current: i + 1,
            total,
        });

        // Retry loop with backoff
        let mut retries = 0;
        if i < 3 {
            eprintln!("[HLTB] Fetching {}: {}", i + 1, title);
        }
        let entry = loop {
            match search_game(&mut hltb, title).await {
                Ok(entry) => break Some(entry),
                Err(HltbError::AuthExpired) => {
                    hltb.refresh_auth().await;
                    retries += 1;
                    if retries > MAX_RETRIES {
                        break None;
                    }
                    continue;
                }
                Err(HltbError::RateLimited) => {
                    retries += 1;
                    if retries > MAX_RETRIES {
                        break None;
                    }
                    sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                    continue;
                }
                Err(HltbError::Network(_)) => {
                    retries += 1;
                    if retries > MAX_RETRIES {
                        break None;
                    }
                    sleep(BACKOFF_INITIAL).await;
                    continue;
                }
                Err(HltbError::Parse(_)) => {
                    break None;
                }
            }
        };

        if let Some(entry) = entry {
            if entry.match_status == "matched" {
                matched_count += 1;
            }
            cache.insert(appid.to_string(), entry);
            // Reset backoff on success
            backoff = BACKOFF_INITIAL;

            // Save cache after every successful fetch
            if let Err(e) = cache::save_hltb_cache(&cache) {
                eprintln!("[HLTB] Cache save error: {e}");
            }
        }

        // Rate limit delay
        if i < total - 1 {
            sleep(REQUEST_DELAY).await;
        }
    }

    // Final save
    let _ = cache::save_hltb_cache(&cache);

    // Emit completion summary
    let total_cached = cache.values().filter(|e| e.match_status == "matched").count();
    let total_games = cache.len();
    let _ = app.emit("hltb-complete", serde_json::json!({
        "matched": total_cached,
        "total": total_games,
        "new_matches": matched_count,
    }));

    Ok(cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_title_basic() {
        assert_eq!(normalize_title("DARK SOULS™ III"), "dark souls iii");
        assert_eq!(normalize_title("The Witcher® 3"), "the witcher 3");
        assert_eq!(normalize_title("Skyrim Special Edition"), "skyrim");
        assert_eq!(normalize_title("Fallout 4 Game of the Year Edition"), "fallout 4");
        assert_eq!(normalize_title("DOOM Eternal Deluxe Edition"), "doom eternal");
    }

    #[test]
    fn test_normalize_title_edge_cases() {
        assert_eq!(normalize_title(""), "");
        assert_eq!(normalize_title("  Portal  2  "), "portal 2");
        assert_eq!(normalize_title("Half-Life 2"), "half-life 2");
        assert_eq!(normalize_title("Final Fantasy VII Remake - Digital Edition"), "final fantasy vii remake");
    }

    #[test]
    fn test_seconds_to_hours() {
        assert_eq!(seconds_to_hours(0), None);
        assert_eq!(seconds_to_hours(-100), None);
        assert_eq!(seconds_to_hours(3600), Some(1.0));
        assert_eq!(seconds_to_hours(5400), Some(1.5));
        assert_eq!(seconds_to_hours(45000), Some(12.5));
    }

    #[test]
    fn test_similarity() {
        assert!((similarity("dark souls iii", "dark souls iii") - 1.0).abs() < 0.01);
        assert!(similarity("dark souls iii", "dark souls 3") > 0.7);
        assert!(similarity("portal", "portal") > 0.99);
        assert!(similarity("the witcher 3", "completely different game") < 0.5);
    }

    #[test]
    fn test_similarity_threshold() {
        // After normalization both become comparable
        let a = normalize_title("The Elder Scrolls V: Skyrim Special Edition");
        let b = normalize_title("The Elder Scrolls V: Skyrim");
        assert!(similarity(&a, &b) > 0.7);

        // Exact match after normalization
        let c = normalize_title("DARK SOULS™ III");
        let d = normalize_title("Dark Souls III");
        assert!(similarity(&c, &d) > 0.99);

        // Different games should be low
        assert!(similarity("portal 2", "call of duty") < 0.5);
    }
}
