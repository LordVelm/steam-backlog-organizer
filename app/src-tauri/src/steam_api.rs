use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::Emitter;
use tokio::time::sleep;

const STEAM_API_BASE: &str = "https://api.steampowered.com";
const STEAM_STORE_API: &str = "https://store.steampowered.com/api/appdetails";
const API_DELAY: Duration = Duration::from_millis(500);
const STORE_DELAY: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedGame {
    pub appid: u64,
    pub name: String,
    pub playtime_hours: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub achievements: Option<AchievementInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementInfo {
    pub total: u32,
    pub achieved: u32,
    pub percentage: f64,
    #[serde(default)]
    pub names_achieved: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreDetails {
    #[serde(rename = "type")]
    pub app_type: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

// -- Steam Web API response types --

#[derive(Deserialize)]
struct OwnedGamesResponse {
    response: OwnedGamesInner,
}

#[derive(Deserialize)]
struct OwnedGamesInner {
    #[serde(default)]
    games: Vec<RawOwnedGame>,
}

#[derive(Deserialize)]
struct RawOwnedGame {
    appid: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    playtime_forever: u64,
}

#[derive(Deserialize)]
struct AchievementsResponse {
    playerstats: Option<AchievementsInner>,
}

#[derive(Deserialize)]
struct AchievementsInner {
    #[serde(default)]
    achievements: Vec<RawAchievement>,
}

#[derive(Deserialize)]
struct RawAchievement {
    #[serde(default)]
    apiname: String,
    #[serde(default)]
    achieved: u32,
}

/// Resolve a Steam vanity URL name to a 64-bit Steam ID.
pub async fn resolve_vanity_url(
    client: &Client,
    api_key: &str,
    vanity_name: &str,
) -> Result<String, String> {
    let url = format!(
        "{STEAM_API_BASE}/ISteamUser/ResolveVanityURL/v0001/\
         ?key={api_key}&vanityurl={vanity_name}"
    );

    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to resolve vanity URL: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse vanity URL response: {e}"))?;

    let success = resp["response"]["success"].as_u64().unwrap_or(0);
    if success == 1 {
        if let Some(steam_id) = resp["response"]["steamid"].as_str() {
            return Ok(steam_id.to_string());
        }
    }

    Err(format!("Could not resolve '{}' to a Steam ID. Check the spelling or use your 64-bit Steam ID instead.", vanity_name))
}

/// Fetch all owned games for a Steam user.
pub async fn get_owned_games(
    client: &Client,
    api_key: &str,
    steam_id: &str,
) -> Result<Vec<OwnedGame>, String> {
    let url = format!(
        "{STEAM_API_BASE}/IPlayerService/GetOwnedGames/v0001/\
         ?key={api_key}&steamid={steam_id}&include_appinfo=1\
         &include_played_free_games=1&format=json"
    );

    let resp: OwnedGamesResponse = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch owned games: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse owned games: {e}"))?;

    let games: Vec<OwnedGame> = resp
        .response
        .games
        .into_iter()
        .map(|g| OwnedGame {
            appid: g.appid,
            name: g.name,
            playtime_hours: g.playtime_forever as f64 / 60.0,
            achievements: None,
        })
        .collect();

    Ok(games)
}

/// Fetch achievements for a single game. Returns None if the game has no achievements.
pub async fn get_player_achievements(
    client: &Client,
    api_key: &str,
    steam_id: &str,
    app_id: u64,
) -> Option<AchievementInfo> {
    let url = format!(
        "{STEAM_API_BASE}/ISteamUserStats/GetPlayerAchievements/v0001/\
         ?appid={app_id}&key={api_key}&steamid={steam_id}"
    );

    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    let data: AchievementsResponse = resp.json().await.ok()?;
    let stats = data.playerstats?;
    let achievements = stats.achievements;

    if achievements.is_empty() {
        return None;
    }

    let total = achievements.len() as u32;
    let achieved: u32 = achievements.iter().map(|a| a.achieved).sum();
    let percentage = if total > 0 {
        (achieved as f64 / total as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };

    let names_achieved: Vec<String> = achievements
        .iter()
        .filter(|a| a.achieved == 1)
        .map(|a| a.apiname.clone())
        .collect();

    Some(AchievementInfo {
        total,
        achieved,
        percentage,
        names_achieved,
    })
}

/// Fetch store details for a single app.
pub async fn fetch_store_details(client: &Client, app_id: u64) -> Option<StoreDetails> {
    let url = format!("{STEAM_STORE_API}?appids={app_id}");

    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    let data: HashMap<String, serde_json::Value> = resp.json().await.ok()?;
    let app_data = data.get(&app_id.to_string())?;

    if !app_data.get("success")?.as_bool()? {
        return None;
    }

    let info = app_data.get("data")?;

    let app_type = info
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let genres: Vec<String> = info
        .get("genres")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|g| g.get("description").and_then(|d| d.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let categories: Vec<String> = info
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("description").and_then(|d| d.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    Some(StoreDetails {
        app_type,
        genres,
        categories,
    })
}

/// Progress event emitted during long-running sync operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncProgress {
    pub step: String,
    pub current: usize,
    pub total: usize,
}

/// Fetch store details for multiple apps with rate limiting.
/// Returns a map of newly fetched details (caller merges into cache).
pub async fn fetch_store_details_batch(
    client: &Client,
    app_ids: &[u64],
    already_cached: &HashSet<String>,
    app: Option<&tauri::AppHandle>,
    cancel: Option<&AtomicBool>,
) -> Result<HashMap<String, StoreDetails>, String> {
    let to_fetch: Vec<u64> = app_ids
        .iter()
        .filter(|id| !already_cached.contains(&id.to_string()))
        .copied()
        .collect();

    let mut results = HashMap::new();
    let total = to_fetch.len();
    for (i, app_id) in to_fetch.iter().enumerate() {
        if cancel.map_or(false, |c| c.load(Ordering::SeqCst)) {
            break;
        }

        if let Some(handle) = app {
            let _ = handle.emit("sync-progress", SyncProgress {
                step: "Fetching store details".into(),
                current: i + 1,
                total,
            });
        }

        if let Some(details) = fetch_store_details(client, *app_id).await {
            results.insert(app_id.to_string(), details);
        }

        if i < total - 1 {
            sleep(STORE_DELAY).await;
        }
    }

    Ok(results)
}

/// Fetch achievements for all games with rate limiting.
pub async fn fetch_all_achievements(
    client: &Client,
    api_key: &str,
    steam_id: &str,
    games: &mut [OwnedGame],
    app: Option<&tauri::AppHandle>,
    cancel: Option<&AtomicBool>,
) {
    let total = games.len();
    for (i, game) in games.iter_mut().enumerate() {
        if cancel.map_or(false, |c| c.load(Ordering::SeqCst)) {
            break;
        }

        if let Some(handle) = app {
            let _ = handle.emit("sync-progress", SyncProgress {
                step: "Fetching achievements".into(),
                current: i + 1,
                total,
            });
        }

        game.achievements =
            get_player_achievements(client, api_key, steam_id, game.appid).await;

        if i < total - 1 {
            sleep(API_DELAY).await;
        }
    }
}
