pub mod cache;
pub mod classifier;
pub mod collections;
pub mod config;
pub mod hltb;
pub mod llm;
pub mod steam_api;

use classifier::{Category, Classification};
use llm::LlmState;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

struct AppState {
    client: Client,
    games: Mutex<Vec<steam_api::OwnedGame>>,
    classifications: Mutex<Vec<Classification>>,
    store_cache: Mutex<HashMap<String, steam_api::StoreDetails>>,
    overrides: Mutex<HashMap<String, String>>,
    hltb_cache: Mutex<HashMap<String, hltb::HltbEntry>>,
    sync_cancelled: Arc<AtomicBool>,
    hltb_cancelled: Arc<AtomicBool>,
    hltb_fetching: Arc<AtomicBool>,
}

// -- Tauri commands --

#[derive(Serialize)]
struct ConfigStatus {
    configured: bool,
    steam_id: Option<String>,
}

#[tauri::command]
fn check_config() -> ConfigStatus {
    match config::load_config() {
        Ok(cfg) => ConfigStatus {
            configured: true,
            steam_id: Some(cfg.steam_id),
        },
        Err(_) => ConfigStatus {
            configured: false,
            steam_id: None,
        },
    }
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, api_key: String, steam_id: String) -> Result<(), String> {
    // Resolve vanity URL if the input isn't a numeric Steam ID
    let resolved_id = if steam_id.chars().all(|c| c.is_ascii_digit()) && steam_id.len() >= 15 {
        steam_id
    } else {
        // Treat as vanity URL name — need API key to resolve
        steam_api::resolve_vanity_url(&state.client, &api_key, &steam_id).await?
    };

    let cfg = config::AppConfig {
        steam_api_key: api_key,
        steam_id: resolved_id,
    };
    config::save_config(&cfg)
}

#[tauri::command]
async fn fetch_library(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<Vec<steam_api::OwnedGame>, String> {
    let cfg = config::load_config()?;
    let cancel = state.sync_cancelled.clone();
    cancel.store(false, Ordering::SeqCst);

    // Try cache first
    if let Some(cached) = cache::load_library_cache(&cfg.steam_id) {
        let mut games_lock = state.games.lock().map_err(|e| e.to_string())?;
        *games_lock = cached.clone();
        return Ok(cached);
    }

    // Fetch from Steam API (no mutex held across await)
    let mut games = steam_api::get_owned_games(&state.client, &cfg.steam_api_key, &cfg.steam_id)
        .await?;

    // Fetch achievements for all games
    steam_api::fetch_all_achievements(
        &state.client,
        &cfg.steam_api_key,
        &cfg.steam_id,
        &mut games,
        Some(&app),
        Some(&cancel),
    )
    .await;

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Cache the result
    let _ = cache::save_library_cache(&cfg.steam_id, &games);

    // Store in state (lock only briefly)
    {
        let mut state_games = state.games.lock().map_err(|e| e.to_string())?;
        *state_games = games.clone();
    }

    Ok(games)
}

#[tauri::command]
async fn fetch_store_details(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let cancel = state.sync_cancelled.clone();

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Clone what we need out of the mutex, then drop the lock
    let app_ids: Vec<u64> = {
        let games = state.games.lock().map_err(|e| e.to_string())?;
        games.iter().map(|g| g.appid).collect()
    };
    let already_cached: std::collections::HashSet<String> = {
        let cache = state.store_cache.lock().map_err(|e| e.to_string())?;
        cache.keys().cloned().collect()
    };

    // Fetch without holding any locks
    let new_details =
        steam_api::fetch_store_details_batch(&state.client, &app_ids, &already_cached, Some(&app), Some(&cancel)).await?;

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Merge results back into state
    {
        let mut store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
        store_cache.extend(new_details);
        cache::save_store_cache(&store_cache)?;
    }

    Ok(())
}

#[tauri::command]
fn cancel_sync(state: State<'_, AppState>) {
    state.sync_cancelled.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn classify_games(state: State<'_, AppState>) -> Result<Vec<Classification>, String> {
    let games = state.games.lock().map_err(|e| e.to_string())?;
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;

    // Load previously saved classifications to preserve stable results for unchanged games
    let saved = cache::load_saved_classifications();

    let results = classifier::classify_all_games(&games, &saved, &overrides, &store_cache);

    // Save to disk
    cache::save_classifications(&results)?;

    let mut classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    *classifications = results.clone();

    Ok(results)
}

#[tauri::command]
fn get_classifications(state: State<'_, AppState>) -> Result<Vec<Classification>, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    Ok(classifications.clone())
}

#[tauri::command]
fn set_override(
    state: State<'_, AppState>,
    appid: String,
    category: String,
) -> Result<(), String> {
    let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    overrides.insert(appid, category);
    cache::save_overrides(&overrides)
}

#[tauri::command]
fn remove_override(state: State<'_, AppState>, appid: String) -> Result<(), String> {
    let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    overrides.remove(&appid);
    cache::save_overrides(&overrides)
}

#[tauri::command]
fn get_overrides(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    Ok(overrides.clone())
}

#[derive(Serialize)]
struct CategorySummary {
    completed: usize,
    in_progress: usize,
    endless: usize,
    not_a_game: usize,
    total: usize,
}

#[tauri::command]
fn get_summary(state: State<'_, AppState>) -> Result<CategorySummary, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    let mut summary = CategorySummary {
        completed: 0,
        in_progress: 0,
        endless: 0,
        not_a_game: 0,
        total: classifications.len(),
    };
    for c in classifications.iter() {
        match c.category {
            Category::Completed => summary.completed += 1,
            Category::InProgress => summary.in_progress += 1,
            Category::Endless => summary.endless += 1,
            Category::NotAGame => summary.not_a_game += 1,
        }
    }
    Ok(summary)
}

#[tauri::command]
fn check_steam_running() -> bool {
    collections::is_steam_running()
}

#[tauri::command]
fn get_steam_accounts() -> Vec<collections::SteamAccount> {
    collections::get_steam_accounts()
}

#[tauri::command]
fn write_to_steam(
    state: State<'_, AppState>,
    account_path: String,
) -> Result<(), String> {
    // Check if Steam is running
    if collections::is_steam_running() {
        return Err("Steam is currently running. Please close Steam before writing collections.".into());
    }

    // Load existing cloud data
    let userdata_path = std::path::PathBuf::from(&account_path);
    let (mut cloud_data, cloud_path) = collections::load_steam_collections(&userdata_path)?;

    // Build categories from current classifications
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    let mut categories: HashMap<String, Vec<u64>> = HashMap::new();
    categories.insert("COMPLETED".into(), Vec::new());
    categories.insert("IN_PROGRESS".into(), Vec::new());
    categories.insert("ENDLESS".into(), Vec::new());
    categories.insert("NOT_A_GAME".into(), Vec::new());

    for c in classifications.iter() {
        let cat_key = c.category.to_string();
        categories.entry(cat_key).or_default().push(c.appid);
    }

    // Write
    collections::write_collections_to_steam(&mut cloud_data, &cloud_path, &categories)
}

// -- AI / LLM commands (bundled llama-server) --

#[tauri::command]
async fn check_ai_setup(app: tauri::AppHandle) -> Result<llm::SetupStatus, String> {
    let data_dir = llm::get_data_dir(&app);
    Ok(llm::check_setup(&data_dir))
}

#[tauri::command]
async fn setup_ai(app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    let status = llm::check_setup(&data_dir);

    if !status.server_ready {
        llm::download_server(&data_dir, &app).await?;
    }
    if !status.model_ready {
        llm::download_model(&data_dir, &app).await?;
    }

    Ok(())
}

#[tauri::command]
fn get_gpu_status(
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
) -> llm::GpuStatus {
    let data_dir = llm::get_data_dir(&app);
    llm::get_gpu_status(&data_dir, &state)
}

#[tauri::command]
fn set_gpu_enabled(
    enabled: bool,
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    llm::set_force_cpu(&state, !enabled, &data_dir);
    // Restart server with new setting
    llm::stop_server(&state);
    llm::start_server(&data_dir, &state)
}

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct RecommendRequest {
    message: String,
    #[serde(default)]
    history: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct RecommendResponse {
    picks: Vec<llm::GameRecommendation>,
    used_llm: bool,
    message: String,
}

/// Strip any JSON that the LLM leaked into its message text.
/// The model sometimes prepends or appends raw JSON syntax around conversational text.
fn clean_llm_message(message: &str) -> String {
    // Patterns that indicate leaked JSON
    let json_patterns = [
        " - {\"", " - [{", " - \"picks\"", " - \"message\"", " - []",
        "[{\"", "{\"message\"", "{\"picks\"",
        "\"picks\":", "\"message\":",
    ];

    let mut result = message.to_string();

    // Step 1: Strip JSON from the beginning (e.g. '{"picks": []} actual message here')
    // Try to find where a JSON object/array at the start ends
    if result.starts_with('{') || result.starts_with('[') {
        // Find the end of the leading JSON structure
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;
        let mut end_idx = 0;
        for (i, ch) in result.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == '{' || ch == '[' {
                depth += 1;
            } else if ch == '}' || ch == ']' {
                depth -= 1;
                if depth == 0 {
                    end_idx = i + 1;
                    break;
                }
            }
        }
        if end_idx > 0 && end_idx < result.len() {
            // There's text after the JSON — use that as the message
            result = result[end_idx..].trim().to_string();
        } else if end_idx > 0 {
            // The entire message is JSON — try to extract "message" field from it
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result[..end_idx]) {
                if let Some(msg) = parsed.get("message").and_then(|m| m.as_str()) {
                    result = msg.to_string();
                }
            }
        }
    }

    // Step 2: Strip JSON from the end
    let mut earliest_cut = result.len();
    for pattern in &json_patterns {
        if let Some(idx) = result.find(pattern) {
            earliest_cut = earliest_cut.min(idx);
        }
    }
    let cleaned = result[..earliest_cut]
        .trim_end_matches(|c: char| c == '-' || c == ' ' || c == '\n' || c == ',');

    let final_msg = if cleaned.is_empty() {
        result.trim().to_string()
    } else {
        cleaned.to_string()
    };

    // Strip markdown formatting (bold **text** → text, *text* → text)
    let mut out = final_msg;
    while out.contains("**") {
        out = out.replacen("**", "", 2);
    }
    while out.contains("__") {
        out = out.replacen("__", "", 2);
    }
    out
}

/// Validate picks against the real game library — fix appids/titles, drop unverifiable picks.
fn validate_picks(
    raw_picks: Vec<llm::GameRecommendation>,
    classifications: &[Classification],
) -> Vec<llm::GameRecommendation> {
    raw_picks
        .into_iter()
        .filter_map(|mut pick| {
            if let Some(real_game) = classifications.iter().find(|c| c.appid == pick.appid) {
                pick.title = real_game.name.clone();
                return Some(pick);
            }
            let pick_title_lower = pick.title.to_lowercase();
            if let Some(real_game) = classifications.iter().find(|c| {
                c.name.to_lowercase() == pick_title_lower
            }) {
                pick.appid = real_game.appid;
                pick.title = real_game.name.clone();
                return Some(pick);
            }
            if pick_title_lower.len() >= 8 {
                if let Some(real_game) = classifications.iter().find(|c| {
                    let name_lower = c.name.to_lowercase();
                    name_lower.starts_with(&pick_title_lower)
                        || pick_title_lower.starts_with(&name_lower)
                }) {
                    pick.appid = real_game.appid;
                    pick.title = real_game.name.clone();
                    return Some(pick);
                }
            }
            None
        })
        .collect()
}

/// Robustly parse an LLM response that may be clean JSON, malformed JSON, or
/// a mix of conversational text and JSON. Returns (message, picks).
fn parse_llm_response(response: &str) -> (String, Vec<llm::GameRecommendation>) {
    // Strategy 1: Try clean JSON parse
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response) {
        let raw_message = parsed
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let picks = parsed
            .get("picks")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| serde_json::from_value::<llm::GameRecommendation>(p.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // If we got a clean parse with picks, great
        if !picks.is_empty() {
            return (clean_llm_message(&raw_message), picks);
        }

        // Clean parse but no picks — try salvaging from message text
        let message = clean_llm_message(&raw_message);
        if let Some(salvaged) = extract_picks_from_message(&raw_message) {
            return (message, salvaged);
        }
        return (message, Vec::new());
    }

    // Strategy 2: JSON parse failed — try to find a JSON object anywhere in the text
    // The model sometimes outputs: "conversational text {"message": "...", "picks": [...]}"
    if let Some(obj_start) = response.find("{\"") {
        let json_candidate = &response[obj_start..];
        // Try to fix common JSON errors (trailing commas, double braces)
        let fixed = fix_json(json_candidate);
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&fixed) {
            let inner_message = parsed
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let picks = parsed
                .get("picks")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| serde_json::from_value::<llm::GameRecommendation>(p.clone()).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // Use the text before the JSON as the message if inner message is empty
            let pre_json = clean_llm_message(&response[..obj_start]);
            let message = if !inner_message.is_empty() {
                clean_llm_message(&inner_message)
            } else if !pre_json.is_empty() {
                pre_json
            } else {
                String::new()
            };
            return (message, picks);
        }
    }

    // Strategy 3: Try to extract picks from any JSON array in the text
    let message = clean_llm_message(response);
    let picks = extract_picks_from_message(response).unwrap_or_default();
    (message, picks)
}

/// Attempt to fix common JSON errors from the LLM (e.g. double braces, trailing commas).
fn fix_json(input: &str) -> String {
    let mut s = input.to_string();
    // Fix double closing braces: }}} → }}
    while s.contains("}}}") {
        s = s.replace("}}}", "}}");
    }
    // Fix trailing commas before closing brackets/braces
    loop {
        let new = s.replace(",]", "]").replace(",}", "}");
        if new == s {
            break;
        }
        s = new;
    }
    s
}

/// Try to extract game picks from JSON embedded in the LLM's message text.
/// Handles multiple formats the model might produce:
/// 1. Raw array: [{"appid": 123, ...}]
/// 2. Nested object: {"message": "...", "picks": [...]}
fn extract_picks_from_message(message: &str) -> Option<Vec<llm::GameRecommendation>> {
    // Try to find a nested JSON object with a "picks" field
    if let Some(obj_start) = message.find("{\"message\"").or_else(|| message.find("{\"picks\"")) {
        let json_candidate = &message[obj_start..];
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_candidate) {
            if let Some(picks_arr) = parsed.get("picks").and_then(|p| p.as_array()) {
                let picks: Vec<llm::GameRecommendation> = picks_arr
                    .iter()
                    .filter_map(|p| serde_json::from_value(p.clone()).ok())
                    .collect();
                if !picks.is_empty() {
                    return Some(picks);
                }
            }
        }
    }

    // Try to find a raw JSON array of picks
    let start = message.find("[{")?;
    let end = message.rfind("}]").map(|i| i + 2)?;
    if end <= start {
        return None;
    }
    let json_str = &message[start..end];
    let picks: Vec<llm::GameRecommendation> = serde_json::from_str(json_str).ok()?;
    if picks.is_empty() {
        return None;
    }
    Some(picks)
}

/// Build candidate summaries for AI inference, enriched with HLTB data.
fn build_candidate_summaries(
    candidates: &[&Classification],
    games: &[steam_api::OwnedGame],
    store_cache: &HashMap<String, steam_api::StoreDetails>,
    hltb_cache: &HashMap<String, hltb::HltbEntry>,
) -> Vec<serde_json::Value> {
    let mut summaries = Vec::new();
    for c in candidates.iter().take(40) {
        let playtime = games
            .iter()
            .find(|g| g.appid == c.appid)
            .map(|g| g.playtime_hours)
            .unwrap_or(0.0);

        let store = store_cache.get(&c.appid.to_string());
        let genres = store.map(|s| s.genres.join(", ")).unwrap_or_default();
        let categories = store.map(|s| s.categories.join(", ")).unwrap_or_default();

        let hltb = hltb_cache.get(&c.appid.to_string());
        let main_story_hours = hltb.and_then(|h| h.main_story_hours);
        let time_remaining = main_story_hours.map(|msh| ((msh - playtime).max(0.0) * 10.0).round() / 10.0);

        let mut summary = serde_json::json!({
            "appid": c.appid,
            "title": c.name,
            "category": c.category.to_string(),
            "playtime_hours": playtime,
            "genres": genres,
            "store_tags": categories,
        });

        // Include HLTB data compactly when available
        if let Some(hours) = main_story_hours {
            summary["hltb_hours"] = serde_json::json!(hours);
        }
        if let Some(remaining) = time_remaining {
            summary["hours_left"] = serde_json::json!(remaining);
        }

        summaries.push(summary);
    }
    summaries
}

#[tauri::command]
async fn get_recommendations(
    state: State<'_, AppState>,
    llm_state: State<'_, LlmState>,
    app: tauri::AppHandle,
    request: RecommendRequest,
) -> Result<RecommendResponse, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?.clone();
    let games = state.games.lock().map_err(|e| e.to_string())?.clone();
    let hltb_cache = state.hltb_cache.lock().map_err(|e| e.to_string())?.clone();

    // Step 1: Deterministic candidate filtering
    let candidates: Vec<&Classification> = classifications
        .iter()
        .filter(|c| c.category == Category::InProgress || c.category == Category::Endless)
        .collect();

    if candidates.is_empty() {
        return Ok(RecommendResponse {
            picks: Vec::new(),
            used_llm: false,
            message: "You don't have any games in your backlog or endless categories yet.".into(),
        });
    }

    // Build a compact profile of candidates (max 40 for LLM context)
    let candidate_summaries = build_candidate_summaries(&candidates, &games, &store_cache, &hltb_cache);

    // Step 2: Check if AI is set up
    let data_dir = llm::get_data_dir(&app);
    let setup = llm::check_setup(&data_dir);
    let ai_available = setup.model_ready && setup.server_ready;

    if ai_available {
        // AI is downloaded — always use it, never fall back to rules
        llm::start_server(&data_dir, &llm_state)
            .map_err(|e| format!("Failed to start AI engine: {e}"))?;
        llm::wait_for_server().await
            .map_err(|e| format!("AI engine not responding: {e}"))?;

        let candidates_json =
            serde_json::to_string_pretty(&candidate_summaries).unwrap_or_default();

        // Convert history to LLM chat messages (limit to last 6 messages
        // to keep context manageable for the local model)
        let history_start = request.history.len().saturating_sub(6);
        let chat_history: Vec<(String, String)> = request
            .history[history_start..]
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();

        let response_text = llm::run_recommendation_inference(
            &candidates_json,
            &request.message,
            &chat_history,
        ).await?;

        // Parse the LLM response — handles clean JSON and various malformed outputs
        let (message, raw_picks) = parse_llm_response(&response_text);
        let validated_picks = validate_picks(raw_picks, &classifications);

        return Ok(RecommendResponse {
            picks: validated_picks,
            used_llm: true,
            message,
        });
    }

    // Step 3: Rules-based fallback — only used when AI model is NOT downloaded
    let fallback: Vec<llm::GameRecommendation> = candidates
        .iter()
        .take(40)
        .map(|c| {
            let playtime = games
                .iter()
                .find(|g| g.appid == c.appid)
                .map(|g| g.playtime_hours)
                .unwrap_or(0.0);
            (c, playtime)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(c, pt)| {
            let reason = if pt == 0.0 {
                "Unplayed — waiting in your backlog".into()
            } else {
                format!("Started but only {pt:.1}h played — might be worth revisiting")
            };
            llm::GameRecommendation {
                appid: c.appid,
                title: c.name.clone(),
                reason,
            }
        })
        .take(3)
        .collect();

    Ok(RecommendResponse {
        picks: fallback,
        used_llm: false,
        message: "Here are some suggestions from your backlog:".into(),
    })
}

#[derive(Deserialize)]
struct AmbiguityRequest {
    appid: u64,
}

#[derive(Serialize)]
struct AmbiguityResponse {
    suggested_category: String,
    rationale: String,
    used_llm: bool,
}

#[tauri::command]
async fn get_ambiguity_suggestion(
    state: State<'_, AppState>,
    llm_state: State<'_, LlmState>,
    app: tauri::AppHandle,
    request: AmbiguityRequest,
) -> Result<AmbiguityResponse, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?.clone();
    let games = state.games.lock().map_err(|e| e.to_string())?.clone();

    let classification = classifications
        .iter()
        .find(|c| c.appid == request.appid)
        .ok_or("Game not found in classifications")?;

    let game = games
        .iter()
        .find(|g| g.appid == request.appid);

    let store_info = store_cache.get(&request.appid.to_string());

    // Check if AI is set up
    let data_dir = llm::get_data_dir(&app);
    let setup = llm::check_setup(&data_dir);
    if !setup.model_ready || !setup.server_ready {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: format!("AI not set up. Current: {}", classification.reason),
            used_llm: false,
        });
    }

    // Try to start server and run inference
    if let Err(_) = llm::start_server(&data_dir, &llm_state) {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: "Failed to start AI engine. Using rule-based classification.".into(),
            used_llm: false,
        });
    }

    if let Err(_) = llm::wait_for_server().await {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: "AI engine not ready. Using rule-based classification.".into(),
            used_llm: false,
        });
    }

    let playtime = game.map(|g| g.playtime_hours).unwrap_or(0.0);
    let genres = store_info
        .map(|s| s.genres.join(", "))
        .unwrap_or_else(|| "Unknown".into());
    let categories = store_info
        .map(|s| s.categories.join(", "))
        .unwrap_or_else(|| "Unknown".into());

    match llm::run_ambiguity_inference(
        &classification.name,
        &genres,
        &categories,
        playtime,
        &classification.category.to_string(),
        &classification.reason,
    )
    .await
    {
        Ok(response_text) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response_text) {
                let cat = parsed
                    .get("suggested_category")
                    .and_then(|c| c.as_str())
                    .unwrap_or("IN_PROGRESS")
                    .to_string();
                let rationale = parsed
                    .get("rationale")
                    .and_then(|r| r.as_str())
                    .unwrap_or("No rationale provided")
                    .to_string();
                return Ok(AmbiguityResponse {
                    suggested_category: cat,
                    rationale,
                    used_llm: true,
                });
            }
            Ok(AmbiguityResponse {
                suggested_category: classification.category.to_string(),
                rationale: "Failed to parse AI response. Using rule-based.".into(),
                used_llm: false,
            })
        }
        Err(e) => Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: format!("AI error: {e}. Using rule-based."),
            used_llm: false,
        }),
    }
}

// -- HLTB --

#[tauri::command]
fn get_hltb_cache(_state: State<'_, AppState>) -> Result<HashMap<String, hltb::HltbEntry>, String> {
    // Read from disk to always get the latest data, even during background fetch
    Ok(cache::load_hltb_cache())
}

#[tauri::command]
async fn fetch_hltb_data(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Prevent concurrent HLTB fetches
    if state.hltb_fetching.load(Ordering::SeqCst) {
        return Ok(());
    }

    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let existing_cache = state.hltb_cache.lock().map_err(|e| e.to_string())?.clone();
    let cancel = state.hltb_cancelled.clone();
    let fetching = state.hltb_fetching.clone();

    // Reset cancel flag for new fetch
    cancel.store(false, Ordering::SeqCst);
    fetching.store(true, Ordering::SeqCst);

    // Build game list with categories for priority sorting
    let games: Vec<(u64, String, String)> = classifications
        .iter()
        .map(|c| (c.appid, c.name.clone(), c.category.to_string()))
        .collect();

    // Spawn background task — updates are saved to disk by fetch_hltb_batch.
    // Frontend re-reads via get_hltb_cache after hltb-complete event.
    let app_handle = app.clone();
    tokio::spawn(async move {
        match hltb::fetch_hltb_batch(games, existing_cache, app_handle.clone(), cancel).await {
            Ok(new_cache) => {
                let app_state: tauri::State<AppState> = app_handle.state();
                match app_state.hltb_cache.lock() {
                    Ok(mut guard) => { *guard = new_cache; }
                    Err(e) => eprintln!("[HLTB] Failed to update cache: {e}"),
                };
            }
            Err(e) => {
                eprintln!("HLTB fetch error: {e}");
            }
        }
        fetching.store(false, Ordering::SeqCst);
    });

    Ok(())
}

// -- Export/Import --

#[tauri::command]
fn export_json(state: State<'_, AppState>) -> Result<String, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&*classifications)
        .map_err(|e| format!("Failed to serialize: {e}"))
}

pub fn run() {
    // Load persisted data
    let store_cache = cache::load_store_cache();
    let overrides = cache::load_overrides();
    let hltb_cache = cache::load_hltb_cache();
    let saved_classifications: Vec<Classification> =
        cache::load_saved_classifications().into_values().collect();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            client: Client::new(),
            games: Mutex::new(Vec::new()),
            classifications: Mutex::new(saved_classifications),
            store_cache: Mutex::new(store_cache),
            overrides: Mutex::new(overrides),
            hltb_cache: Mutex::new(hltb_cache),
            sync_cancelled: Arc::new(AtomicBool::new(false)),
            hltb_cancelled: Arc::new(AtomicBool::new(false)),
            hltb_fetching: Arc::new(AtomicBool::new(false)),
        })
        .manage(LlmState {
            server_process: Mutex::new(None),
            force_cpu: Mutex::new(false),
        })
        .setup(|app| {
            // Load persisted GPU preference
            let data_dir = llm::get_data_dir(app.handle());
            let force_cpu = llm::load_force_cpu(&data_dir);
            let llm_state = app.state::<LlmState>();
            if let Ok(mut guard) = llm_state.force_cpu.lock() {
                *guard = force_cpu;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_config,
            save_config,
            fetch_library,
            fetch_store_details,
            classify_games,
            get_classifications,
            set_override,
            remove_override,
            get_overrides,
            get_summary,
            check_steam_running,
            get_steam_accounts,
            write_to_steam,
            check_ai_setup,
            setup_ai,
            get_gpu_status,
            set_gpu_enabled,
            cancel_sync,
            get_recommendations,
            get_ambiguity_suggestion,
            get_hltb_cache,
            fetch_hltb_data,
            export_json,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            let llm_state = app_handle.state::<LlmState>();
            llm::stop_server(&llm_state);
            // Save HLTB cache on exit
            {
                let app_state: tauri::State<AppState> = app_handle.state();
                match app_state.hltb_cache.lock() {
                    Ok(cache) => { let _ = cache::save_hltb_cache(&cache); }
                    Err(_) => eprintln!("[HLTB] Could not save cache on exit (mutex poisoned)"),
                };
            }
        }
    });
}
