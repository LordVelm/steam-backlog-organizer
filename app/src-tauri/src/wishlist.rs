//! Steam wishlist via IWishlistService/GetWishlist (keyless — public profiles
//! only). Response carries only appid/priority/date_added — names come from
//! the local catalog join.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WISHLIST_CACHE_MAX_AGE_SECS: u64 = 60 * 60; // 1 hour

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishlistItem {
    pub appid: u32,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub date_added: u64,
}

#[derive(Debug)]
pub enum WishlistError {
    /// Profile private or wishlist hidden (empty response is indistinguishable
    /// from a genuinely empty wishlist — treat both as this).
    EmptyOrPrivate,
    Http(String),
}

impl std::fmt::Display for WishlistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WishlistError::EmptyOrPrivate => write!(f, "WISHLIST_EMPTY_OR_PRIVATE"),
            WishlistError::Http(e) => write!(f, "WISHLIST_HTTP: {e}"),
        }
    }
}

#[derive(Deserialize)]
struct WishlistResponse {
    response: WishlistInner,
}

#[derive(Deserialize)]
struct WishlistInner {
    #[serde(default)]
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    appid: u32,
    #[serde(default)]
    priority: u32,
    #[serde(default)]
    date_added: u64,
}

/// Deliberately keyless: the endpoint works without a key for public profiles,
/// and keeping the key out of the URL keeps it out of reqwest error strings
/// (which surface in the UI).
pub async fn fetch_wishlist(
    client: &Client,
    steam_id: &str,
) -> Result<Vec<WishlistItem>, WishlistError> {
    let url = format!(
        "https://api.steampowered.com/IWishlistService/GetWishlist/v1/?steamid={steam_id}"
    );
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| WishlistError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(WishlistError::Http(format!("status {}", resp.status())));
    }
    let data: WishlistResponse = resp
        .json()
        .await
        .map_err(|e| WishlistError::Http(format!("parse: {e}")))?;

    if data.response.items.is_empty() {
        return Err(WishlistError::EmptyOrPrivate);
    }
    Ok(data
        .response
        .items
        .into_iter()
        .map(|i| WishlistItem {
            appid: i.appid,
            priority: i.priority,
            date_added: i.date_added,
        })
        .collect())
}

// -- 1h disk cache ----------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct WishlistCache {
    fetched_at: u64,
    #[serde(default)]
    steam_id: String,
    items: Vec<WishlistItem>,
}

fn cache_file() -> PathBuf {
    crate::config::cache_dir().join("wishlist.json")
}

pub fn load_cached(steam_id: &str) -> Option<Vec<WishlistItem>> {
    let data = std::fs::read_to_string(cache_file()).ok()?;
    let cache: WishlistCache = serde_json::from_str(&data).ok()?;
    if cache.steam_id != steam_id {
        return None; // account switched — never serve another user's wishlist
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if now.saturating_sub(cache.fetched_at) > WISHLIST_CACHE_MAX_AGE_SECS {
        return None;
    }
    Some(cache.items)
}

pub fn save_cache(steam_id: &str, items: &[WishlistItem]) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = WishlistCache {
        fetched_at: now,
        steam_id: steam_id.to_string(),
        items: items.to_vec(),
    };
    if let Ok(data) = serde_json::to_string_pretty(&cache) {
        let _ = std::fs::create_dir_all(crate::config::cache_dir());
        let _ = std::fs::write(cache_file(), data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_response_shape() {
        let json = r#"{"response":{"items":[
            {"appid":667970,"priority":3,"date_added":1672362766},
            {"appid":2109770,"priority":13,"date_added":1718574744},
            {"appid":823500}
        ]}}"#;
        let parsed: WishlistResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.response.items.len(), 3);
        assert_eq!(parsed.response.items[0].appid, 667970);
        assert_eq!(parsed.response.items[2].priority, 0); // defaults
    }

    #[test]
    fn empty_response_is_private_or_empty() {
        let json = r#"{"response":{}}"#;
        let parsed: WishlistResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.response.items.is_empty());
    }
}
