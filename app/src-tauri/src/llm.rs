use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const MODEL_URL: &str = "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q8_0.gguf";
const MODEL_FILENAME: &str = "Qwen3.5-9B-Q8_0.gguf";
pub const LLAMA_SERVER_PORT: u16 = 39282; // Different port from debt planner (39281)

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub model_ready: bool,
    pub server_ready: bool,
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

/// Strip `<think>...</think>` blocks that Qwen3.x models may emit.
fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            result = format!("{}{}", &result[..start], &result[end + 8..]);
        } else {
            // Unclosed <think> tag — strip from <think> to end
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Extract a JSON response from reasoning_content when the model puts its answer there.
/// Scans for the last JSON object containing "message" or "picks" keys.
fn extract_json_from_reasoning(reasoning: &str) -> String {
    // Look for JSON objects in the reasoning text, starting from the end
    // (the final answer is usually at the end of the reasoning)
    let mut best_json = String::new();

    for (i, _) in reasoning.match_indices('{') {
        let candidate = &reasoning[i..];
        // Try to find matching closing brace using depth tracking
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;
        let mut end_idx = None;
        for (j, ch) in candidate.char_indices() {
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
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(j + 1);
                    break;
                }
            }
        }
        if let Some(end) = end_idx {
            let json_str = &candidate[..end];
            // Check if this looks like our expected response format
            if (json_str.contains("\"message\"") || json_str.contains("\"picks\""))
                && serde_json::from_str::<serde_json::Value>(json_str).is_ok()
            {
                best_json = json_str.to_string();
            }
        }
    }

    best_json
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

pub fn get_model_path(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join(MODEL_FILENAME)
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
    SetupStatus {
        model_ready: get_model_path(data_dir).exists(),
        server_ready: get_server_path(data_dir).exists(),
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

pub async fn download_model(data_dir: &Path, app: &AppHandle) -> Result<(), String> {
    // Clean up old model files to free disk space
    let models_dir = data_dir.join("models");
    if models_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".gguf") && name != MODEL_FILENAME {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    let model_path = get_model_path(data_dir);
    download_to_file(MODEL_URL, &model_path, app, "Downloading AI model").await
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
    let model_path = get_model_path(data_dir);
    let bin_dir = get_server_dir(data_dir);

    if !server_path.exists() || !model_path.exists() {
        return Err("AI not set up yet. Please download the AI model first.".to_string());
    }

    // Use GPU if we downloaded the CUDA build and not forced to CPU
    let force_cpu = state.force_cpu.lock().map_err(|e| e.to_string())?;
    let gpu_layers = if has_cuda_build(data_dir) && !*force_cpu { "99" } else { "0" };
    drop(force_cpu);

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
        .arg("16384")
        .arg("--cont-batching")
        .arg("--jinja")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

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

    run_inference_chat(&system_prompt, user_message, history).await
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
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "http://127.0.0.1:{}/v1/chat/completions",
        LLAMA_SERVER_PORT
    );

    // Build messages array: system + history + current user message
    let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];

    for (role, content) in history {
        messages.push(serde_json::json!({"role": role, "content": content}));
    }

    messages.push(serde_json::json!({"role": "user", "content": user_message}));

    let body = serde_json::json!({
        "model": "local",
        "messages": messages,
        "temperature": 0.8,
        "enable_thinking": false,
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

    let message = &response["choices"][0]["message"];

    // Primary: use "content" field
    let content = message["content"].as_str().unwrap_or("").to_string();

    // Fallback: if content is empty but reasoning_content has data (Qwen3.x thinking mode),
    // look for JSON in the reasoning output
    let raw = if content.is_empty() {
        let reasoning = message["reasoning_content"].as_str().unwrap_or("");
        extract_json_from_reasoning(reasoning)
    } else {
        content
    };

    if raw.is_empty() {
        return Err("AI returned empty response. Try again.".to_string());
    }

    let cleaned = strip_think_tags(&raw)
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
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "http://127.0.0.1:{}/v1/chat/completions",
        LLAMA_SERVER_PORT
    );

    let body = serde_json::json!({
        "model": "local",
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "enable_thinking": false,
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

    let message = &response["choices"][0]["message"];
    let content = message["content"].as_str().unwrap_or("").to_string();

    let raw = if content.is_empty() {
        let reasoning = message["reasoning_content"].as_str().unwrap_or("");
        extract_json_from_reasoning(reasoning)
    } else {
        content
    };

    if raw.is_empty() {
        return Err("AI returned empty response. Try again.".to_string());
    }

    // Clean thinking tags and markdown code blocks if present
    let cleaned = strip_think_tags(&raw)
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    Ok(cleaned)
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
