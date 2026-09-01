use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

pub const LLAMA_SERVER_PORT: u16 = 39282; // Different port from debt planner (39281)

/// A downloadable model tier. "No AI" is a UI concept, not a table row —
/// the taste engine keeps the whole app functional with zero tiers installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTier {
    pub id: &'static str,
    pub label: &'static str,
    #[serde(skip)]
    pub url: &'static str,
    #[serde(skip)]
    pub filename: &'static str,
    pub size_bytes: u64,
    pub min_vram_mb: u64,
    pub min_ram_mb_cpu: u64,
    #[serde(skip)]
    pub ctx: u32,
}

pub const MODEL_TIERS: &[ModelTier] = &[
    ModelTier {
        id: "qwen3.5-4b-q4",
        label: "Standard — Qwen3.5 4B",
        url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
        filename: "Qwen3.5-4B-Q4_K_M.gguf",
        size_bytes: 2_740_000_000,
        min_vram_mb: 4_096,
        min_ram_mb_cpu: 12_288,
        ctx: 8192,
    },
    ModelTier {
        id: "qwen3-8b-q4",
        label: "Plus — Qwen3 8B",
        url: "https://huggingface.co/unsloth/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
        filename: "Qwen3-8B-Q4_K_M.gguf",
        size_bytes: 5_030_000_000,
        min_vram_mb: 8_192,
        min_ram_mb_cpu: 16_384,
        ctx: 16384,
    },
    ModelTier {
        id: "qwen2.5-14b-q8",
        label: "Max — Qwen2.5 14B",
        url: "https://huggingface.co/bartowski/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q8_0.gguf",
        filename: "Qwen2.5-14B-Instruct-Q8_0.gguf",
        size_bytes: 15_700_000_000,
        min_vram_mb: 20_480,
        min_ram_mb_cpu: 32_768,
        ctx: 16384,
    },
];

pub fn tier_by_id(id: &str) -> Option<&'static ModelTier> {
    MODEL_TIERS.iter().find(|t| t.id == id)
}

fn ai_settings_file(data_dir: &Path) -> PathBuf {
    data_dir.join("ai_settings.json")
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AiSettings {
    active_tier: Option<String>,
}

pub fn set_active_tier(data_dir: &Path, tier_id: Option<&str>) {
    let settings = AiSettings {
        active_tier: tier_id.map(String::from),
    };
    if let Ok(data) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::create_dir_all(data_dir);
        let _ = std::fs::write(ai_settings_file(data_dir), data);
    }
}

/// Active tier, with one-time migration: pre-v4 installs have the 14B model on
/// disk but no ai_settings.json — adopt it so nothing re-downloads.
pub fn active_tier(data_dir: &Path) -> Option<&'static ModelTier> {
    let file = ai_settings_file(data_dir);
    if let Ok(data) = std::fs::read_to_string(&file) {
        let settings: AiSettings = serde_json::from_str(&data).unwrap_or_default();
        return settings.active_tier.as_deref().and_then(tier_by_id);
    }
    // Migration: no settings file yet — adopt any already-installed tier
    // (prefer the largest, which is what a legacy install has).
    for tier in MODEL_TIERS.iter().rev() {
        if get_model_path_for(data_dir, tier).exists() {
            set_active_tier(data_dir, Some(tier.id));
            return Some(tier);
        }
    }
    None
}

pub fn installed_tier_ids(data_dir: &Path) -> Vec<&'static str> {
    MODEL_TIERS
        .iter()
        .filter(|t| get_model_path_for(data_dir, t).exists())
        .map(|t| t.id)
        .collect()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub model_ready: bool,
    pub server_ready: bool,
    #[serde(default)]
    pub active_tier: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
}

pub struct LlmState {
    pub server_process: Mutex<Option<Child>>,
    pub force_cpu: Mutex<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    pub gpu_detected: bool,
    pub cuda_build: bool,
    pub using_gpu: bool,
}

/// Game recommendation from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRecommendation {
    pub appid: u64,
    pub title: String,
    pub reason: String,
}

pub fn get_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("failed to get app data dir")
}

pub fn get_model_path_for(data_dir: &Path, tier: &ModelTier) -> PathBuf {
    data_dir.join("models").join(tier.filename)
}

/// Model path for the active tier (None when no tier active/installed).
pub fn get_model_path(data_dir: &Path) -> Option<PathBuf> {
    active_tier(data_dir).map(|t| get_model_path_for(data_dir, t))
}

pub fn get_server_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bin")
}

pub fn get_server_path(data_dir: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    get_server_dir(data_dir).join(name)
}

/// Detect whether an NVIDIA GPU is available by running nvidia-smi
pub fn has_nvidia_gpu() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("nvidia-smi")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("nvidia-smi")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Check if we downloaded the CUDA build (marker file)
pub fn has_cuda_build(data_dir: &Path) -> bool {
    get_server_dir(data_dir).join(".cuda").exists()
}

pub fn check_setup(data_dir: &Path) -> SetupStatus {
    let tier = active_tier(data_dir);
    SetupStatus {
        model_ready: tier
            .map(|t| get_model_path_for(data_dir, t).exists())
            .unwrap_or(false),
        server_ready: get_server_path(data_dir).exists(),
        active_tier: tier.map(|t| t.id.to_string()),
    }
}

async fn download_to_file(
    url: &str,
    dest: &Path,
    app: &AppHandle,
    stage: &str,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = dest.with_extension("tmp");

    let client = reqwest::Client::builder()
        .user_agent("gamekeeper/3.0")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download returned status {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                stage: stage.to_string(),
                downloaded,
                total,
                percent,
            },
        );
    }

    drop(file);
    std::fs::rename(&tmp_path, dest).map_err(|e| e.to_string())?;

    Ok(())
}

fn extract_zip_binaries(zip_path: &Path, bin_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let server_exe = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();

        let should_extract = entry_name.ends_with(server_exe)
            || entry_name.ends_with(".dll")
            || entry_name.ends_with(".so")
            || entry_name.ends_with(".dylib");

        if should_extract {
            let filename = Path::new(&entry_name)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let dest = bin_dir.join(&filename);
            let mut outfile = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    let _ = std::fs::remove_file(zip_path);
    Ok(())
}

pub async fn download_server(data_dir: &Path, app: &AppHandle) -> Result<(), String> {
    let gpu_available = has_nvidia_gpu();
    let use_cuda = gpu_available && cfg!(target_os = "windows");

    let client = reqwest::Client::builder()
        .user_agent("gamekeeper/3.0")
        .build()
        .map_err(|e| e.to_string())?;

    let release: serde_json::Value = client
        .get("https://api.github.com/repos/ggml-org/llama.cpp/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch release info: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse release info: {}", e))?;

    let assets = release["assets"]
        .as_array()
        .ok_or("No assets in release")?;

    let bin_dir = get_server_dir(data_dir);
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    if use_cuda {
        let cuda_asset = assets
            .iter()
            .filter(|a| {
                let name = a["name"].as_str().unwrap_or("").to_lowercase();
                name.contains("win")
                    && name.contains("x64")
                    && name.contains("cuda")
                    && name.ends_with(".zip")
                    && !name.starts_with("cudart")
            })
            .last()
            .ok_or("Could not find CUDA build of llama.cpp")?;

        let cuda_asset_name = cuda_asset["name"].as_str().unwrap_or("");
        let cuda_url = cuda_asset["browser_download_url"]
            .as_str()
            .ok_or("No download URL for CUDA asset")?;

        let cuda_ver_tag: String = cuda_asset_name
            .to_lowercase()
            .split("cuda-")
            .nth(1)
            .unwrap_or("12")
            .split('-')
            .next()
            .unwrap_or("12")
            .to_string();

        let zip_path = bin_dir.join("llama-server.zip");
        download_to_file(cuda_url, &zip_path, app, "Downloading AI engine (GPU)").await?;
        extract_zip_binaries(&zip_path, &bin_dir)?;

        let cudart_asset = assets.iter().find(|a| {
            let name = a["name"].as_str().unwrap_or("").to_lowercase();
            name.starts_with("cudart")
                && name.contains("win")
                && name.contains(&format!("cuda-{}", cuda_ver_tag))
                && name.contains("x64")
                && name.ends_with(".zip")
        });

        if let Some(cudart) = cudart_asset {
            if let Some(cudart_url) = cudart["browser_download_url"].as_str() {
                let cudart_zip = bin_dir.join("cudart.zip");
                download_to_file(cudart_url, &cudart_zip, app, "Downloading CUDA runtime")
                    .await?;
                extract_zip_binaries(&cudart_zip, &bin_dir)?;
            }
        }

        let _ = std::fs::File::create(bin_dir.join(".cuda"));
    } else {
        let cpu_asset = assets
            .iter()
            .find(|a| {
                let name = a["name"].as_str().unwrap_or("").to_lowercase();
                if cfg!(target_os = "windows") {
                    name.contains("win")
                        && name.contains("x64")
                        && (name.contains("cpu") || name.contains("avx2"))
                        && name.ends_with(".zip")
                        && !name.starts_with("cudart")
                } else if cfg!(target_os = "macos") {
                    name.contains("macos") && name.ends_with(".zip")
                } else {
                    name.contains("linux")
                        && name.contains("x64")
                        && name.contains("cpu")
                        && name.ends_with(".zip")
                }
            })
            .ok_or("Could not find a compatible llama.cpp release for this platform")?;

        let cpu_url = cpu_asset["browser_download_url"]
            .as_str()
            .ok_or("No download URL for CPU asset")?;

        let zip_path = bin_dir.join("llama-server.zip");
        download_to_file(cpu_url, &zip_path, app, "Downloading AI engine (CPU)").await?;
        extract_zip_binaries(&zip_path, &bin_dir)?;
    }

    if !get_server_path(data_dir).exists() {
        return Err("llama-server not found in downloaded archive".to_string());
    }

    Ok(())
}

/// Download one tier's model. Tiers coexist on disk — only stale .tmp files
/// from interrupted downloads are cleaned, never other tiers' models.
pub async fn download_model(data_dir: &Path, app: &AppHandle, tier: &ModelTier) -> Result<(), String> {
    let models_dir = data_dir.join("models");
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(".tmp") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let model_path = get_model_path_for(data_dir, tier);
    if !model_path.exists() {
        download_to_file(tier.url, &model_path, app, "Downloading AI model").await?;
    }
    set_active_tier(data_dir, Some(tier.id));
    Ok(())
}

/// Permanently delete one tier's model file. If it was active, activate the
/// largest remaining installed tier (or none).
pub fn delete_model(data_dir: &Path, tier: &ModelTier) -> Result<(), String> {
    let path = get_model_path_for(data_dir, tier);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("delete model: {e}"))?;
    }
    if active_tier(data_dir).map(|t| t.id) == Some(tier.id) {
        let next = MODEL_TIERS
            .iter()
            .rev()
            .find(|t| get_model_path_for(data_dir, t).exists());
        set_active_tier(data_dir, next.map(|t| t.id));
    }
    Ok(())
}

pub fn start_server(data_dir: &Path, state: &LlmState) -> Result<(), String> {
    let mut process_guard = state.server_process.lock().map_err(|e| e.to_string())?;

    // Check if already running
    if let Some(ref mut child) = *process_guard {
        match child.try_wait() {
            Ok(None) => return Ok(()), // still running
            _ => {}                    // exited, will restart
        }
    }

    let server_path = get_server_path(data_dir);
    let tier = active_tier(data_dir)
        .ok_or("AI not set up yet. Please download an AI model first.")?;
    let model_path = get_model_path_for(data_dir, tier);
    let bin_dir = get_server_dir(data_dir);

    if !server_path.exists() || !model_path.exists() {
        return Err("AI not set up yet. Please download the AI model first.".to_string());
    }

    // Use GPU if we downloaded the CUDA build and not forced to CPU
    let force_cpu = state.force_cpu.lock().map_err(|e| e.to_string())?;
    let gpu_layers = if has_cuda_build(data_dir) && !*force_cpu { "99" } else { "0" };
    drop(force_cpu);

    // Keep server stderr in a log so GPU OOM / template errors are diagnosable
    let log = std::fs::File::create(data_dir.join("llama-server.log"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());

    let mut cmd = Command::new(&server_path);
    cmd.current_dir(&bin_dir)
        .arg("-m")
        .arg(&model_path)
        .arg("--port")
        .arg(LLAMA_SERVER_PORT.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("-ngl")
        .arg(gpu_layers)
        .arg("--ctx-size")
        .arg(tier.ctx.to_string())
        .arg("--cont-batching")
        .stdout(Stdio::null())
        .stderr(log);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start llama-server: {}", e))?;

    *process_guard = Some(child);

    Ok(())
}

pub async fn wait_for_server() -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", LLAMA_SERVER_PORT);

    for _ in 0..60 {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    Err("AI engine took too long to start. This may happen on first run while the model loads — try again.".to_string())
}

/// Run inference for game recommendations with full conversation history.
pub async fn run_recommendation_inference(
    candidates_json: &str,
    user_message: &str,
    history: &[(String, String)],
) -> Result<String, String> {
    let system_prompt = format!(
        "You are a gaming expert and critic with the depth of knowledge of someone who writes for \
         Game Informer, IGN, or Kotaku. You've played thousands of games and have strong, genuine \
         opinions about what makes each one worth playing.\n\n\
         You're chatting with someone about their Steam library, helping them decide what to play. \
         Be natural and varied in your responses — avoid starting with the same phrases. \
         Sometimes be enthusiastic, sometimes thoughtful, sometimes direct. Mix up your tone. \
         Give specific, genuine insights about games — mention actual mechanics, story beats, \
         art direction, or design choices rather than generic praise like \"engaging\" or \"fantastic\".\n\n\
         STRICT RULES:\n\
         1. ONLY recommend games from the candidate list below. These are games the user OWNS. \
            Never invent or hallucinate games not on this list.\n\
         2. The \"appid\" in your picks MUST exactly match an appid from the candidate list. \
            Copy appids directly — do not guess or make up appid numbers.\n\
         3. The \"title\" in your picks MUST exactly match the title from the candidate list.\n\
         4. Do NOT recommend games you already recommended earlier in this conversation.\n\
         5. If the user asks about a previous recommendation (\"why?\", \"tell me more\"), \
            respond conversationally with an empty picks array — do not re-recommend it.\n\
         6. If asked for alternatives, pick DIFFERENT games not yet mentioned.\n\
         7. Pay attention to what the user actually asked for. \"Something short\" means low \
            playtime or known short games. \"Something I haven't played\" means 0 playtime_hours.\n\
         8. When the user reacts positively (\"awesome\", \"cool\", \"nice\"), respond \
            enthusiastically and build on it — share more about why that game is great, \
            or suggest something similar. Never respond with just \"...\" or empty filler.\n\
         9. Always write a substantive message, even for conversational replies. \
            A good conversational reply is 1-3 sentences that add value.\n\
         10. Games may include hltb_hours (estimated completion time) and hours_left \
            (time remaining based on playtime). When the user mentions time \
            (\"I have 2 hours\", \"something short\", \"tonight\"), prioritize games \
            where hours_left fits. Mention time naturally. If a game has no hltb_hours, \
            you can still recommend it but note time is unknown.\n\n\
         CANDIDATE LIST (the user's games — only recommend from these):\n{candidates_json}\n\n\
         Respond with ONLY valid JSON:\n\
         {{\"message\": \"Your natural conversational response (1-3 sentences, varied tone)\", \
         \"picks\": [{{\"appid\": 12345, \"title\": \"Exact Title From List\", \
         \"reason\": \"Specific reason tied to the user's request\"}}]}}\n\n\
         - \"picks\" can be [] for conversational replies\n\
         - Include 1-3 picks when recommending\n\
         - Double-check that every appid and title matches the candidate list exactly\n\
         - NEVER mention appids, playtime_hours numbers, or other technical data in the \"message\" field — speak naturally as a human would"
    );

    run_inference_chat(&system_prompt, user_message, history, 0.8).await
}

/// Run inference for ambiguity classification suggestion.
pub async fn run_ambiguity_inference(
    game_name: &str,
    genres: &str,
    categories: &str,
    playtime: f64,
    current_category: &str,
    current_reason: &str,
) -> Result<String, String> {
    let prompt = format!(
        "Classify this Steam game into one of: COMPLETED, IN_PROGRESS, ENDLESS, NOT_A_GAME.\n\n\
         Title: {game_name}\n\
         Genres: {genres}\n\
         Store categories: {categories}\n\
         Playtime: {playtime}h\n\
         Current rule-based classification: {current_category} ({current_reason})\n\n\
         Respond with ONLY valid JSON matching this format:\n\
         {{\"suggested_category\": \"CATEGORY\", \"rationale\": \"Short explanation\"}}"
    );

    run_inference_raw(&prompt).await
}

/// Multi-turn chat inference against the local llama-server.
async fn run_inference_chat(
    system_prompt: &str,
    user_message: &str,
    history: &[(String, String)],
    temperature: f64,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "http://127.0.0.1:{}/v1/chat/completions",
        LLAMA_SERVER_PORT
    );

    // Build messages array: system (when present) + history + current user message
    let mut messages = Vec::new();
    if !system_prompt.is_empty() {
        messages.push(serde_json::json!({"role": "system", "content": system_prompt}));
    }

    for (role, content) in history {
        messages.push(serde_json::json!({"role": role, "content": content}));
    }

    messages.push(serde_json::json!({"role": "user", "content": user_message}));

    let body = serde_json::json!({
        "model": "local",
        "messages": messages,
        "temperature": temperature,
        "response_format": {"type": "json_object"}
    });

    // Wait for server to be idle
    let health_url = format!("http://127.0.0.1:{}/health", LLAMA_SERVER_PORT);
    for _ in 0..30 {
        if let Ok(resp) = client.get(&health_url).send().await {
            if let Ok(health) = resp.json::<serde_json::Value>().await {
                let status = health["status"].as_str().unwrap_or("");
                if status == "ok" || status == "no slot available" {
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let http_response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Inference request failed: {}", e))?;

    let response_text = http_response
        .text()
        .await
        .map_err(|e| format!("Failed to read inference response: {}", e))?;

    let response: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse inference response: {}. Raw: {}", e, response_text))?;

    if let Some(err) = response.get("error") {
        return Err(format!("AI server error: {}", err));
    }

    let raw = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if raw.is_empty() {
        return Err("AI returned empty response. Try again.".to_string());
    }

    let cleaned = strip_think_blocks(&raw)
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    Ok(cleaned)
}

/// Low-level single-turn inference against the local llama-server.
async fn run_inference_raw(prompt: &str) -> Result<String, String> {
    // Same request path as chat, no system prompt / history, deterministic temp
    run_inference_chat("", prompt, &[], 0.1).await
}

/// Taste-profile prose generation (creative temp, system-primed).
pub async fn run_taste_prose_inference(system: &str, facts_json: &str) -> Result<String, String> {
    run_inference_chat(system, facts_json, &[], 0.7).await
}

/// Qwen3.x hybrid-reasoning models can leak `<think>...</think>` blocks into
/// chat completions depending on the template. Strip them defensively.
fn strip_think_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = ""; // unclosed block: drop everything after <think>
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_think_blocks() {
        assert_eq!(strip_think_blocks("hello"), "hello");
        assert_eq!(
            strip_think_blocks("<think>reasoning...</think>{\"a\":1}"),
            "{\"a\":1}"
        );
        assert_eq!(
            strip_think_blocks("a<think>x</think>b<think>y</think>c"),
            "abc"
        );
        assert_eq!(strip_think_blocks("start<think>never closed"), "start");
    }

    #[test]
    fn tier_table_sane() {
        assert_eq!(MODEL_TIERS.len(), 3);
        // ascending by size, ids unique, legacy 14B keeps its exact filename
        assert!(MODEL_TIERS.windows(2).all(|w| w[0].size_bytes < w[1].size_bytes));
        assert_eq!(
            MODEL_TIERS.last().unwrap().filename,
            "Qwen2.5-14B-Instruct-Q8_0.gguf"
        );
        assert!(tier_by_id("qwen3.5-4b-q4").is_some());
        assert!(tier_by_id("nope").is_none());
    }

    fn temp_data_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gk_llm_test_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        dir
    }

    fn fake_model(dir: &Path, tier: &ModelTier) {
        std::fs::write(get_model_path_for(dir, tier), b"gguf").unwrap();
    }

    #[test]
    fn legacy_install_migration_adopts_largest_installed_tier() {
        // Pre-v4 install: 14B gguf on disk, NO ai_settings.json.
        // Must adopt it (never force a 15.7 GB re-download) and persist.
        let dir = temp_data_dir("migrate");
        fake_model(&dir, tier_by_id("qwen2.5-14b-q8").unwrap());

        let adopted = active_tier(&dir).expect("migration must adopt installed tier");
        assert_eq!(adopted.id, "qwen2.5-14b-q8");
        assert!(ai_settings_file(&dir).exists(), "migration must persist");
        // Second call reads the settings file, same answer
        assert_eq!(active_tier(&dir).unwrap().id, "qwen2.5-14b-q8");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_install_has_no_active_tier() {
        let dir = temp_data_dir("fresh");
        assert!(active_tier(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_active_tier_hands_off_to_largest_remaining() {
        let dir = temp_data_dir("delete");
        let small = tier_by_id("qwen3.5-4b-q4").unwrap();
        let big = tier_by_id("qwen2.5-14b-q8").unwrap();
        fake_model(&dir, small);
        fake_model(&dir, big);
        set_active_tier(&dir, Some(big.id));

        delete_model(&dir, big).unwrap();
        assert!(!get_model_path_for(&dir, big).exists());
        assert_eq!(
            active_tier(&dir).map(|t| t.id),
            Some(small.id),
            "deleting the active tier must activate the largest remaining"
        );

        delete_model(&dir, small).unwrap();
        assert!(active_tier(&dir).is_none(), "no tiers left → none active");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_inactive_tier_keeps_active_untouched() {
        let dir = temp_data_dir("delete_inactive");
        let small = tier_by_id("qwen3.5-4b-q4").unwrap();
        let big = tier_by_id("qwen2.5-14b-q8").unwrap();
        fake_model(&dir, small);
        fake_model(&dir, big);
        set_active_tier(&dir, Some(big.id));

        delete_model(&dir, small).unwrap();
        assert_eq!(active_tier(&dir).map(|t| t.id), Some(big.id));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub fn get_gpu_status(data_dir: &Path, state: &LlmState) -> GpuStatus {
    let gpu_detected = has_nvidia_gpu();
    let cuda_build = has_cuda_build(data_dir);
    let force_cpu = state.force_cpu.lock().map(|v| *v).unwrap_or(false);
    GpuStatus {
        gpu_detected,
        cuda_build,
        using_gpu: cuda_build && !force_cpu,
    }
}

pub fn set_force_cpu(state: &LlmState, force: bool, data_dir: &Path) {
    if let Ok(mut guard) = state.force_cpu.lock() {
        *guard = force;
    }
    // Persist preference
    let pref_path = data_dir.join("gpu_preference.json");
    let _ = std::fs::write(&pref_path, if force { "false" } else { "true" });
}

/// Load the persisted GPU preference. Returns true if GPU should be forced off (CPU only).
pub fn load_force_cpu(data_dir: &Path) -> bool {
    let pref_path = data_dir.join("gpu_preference.json");
    match std::fs::read_to_string(&pref_path) {
        Ok(val) => val.trim() == "false",
        Err(_) => false, // Default: GPU enabled (force_cpu = false)
    }
}

pub fn stop_server(state: &LlmState) {
    if let Ok(mut guard) = state.server_process.lock() {
        if let Some(ref mut child) = *guard {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
    }
}

/// Check if the AI server is currently running and responsive.
pub async fn is_server_running() -> bool {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", LLAMA_SERVER_PORT);
    if let Ok(resp) = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        resp.status().is_success()
    } else {
        false
    }
}
