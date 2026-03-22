use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STEAM_BASE: &str = "C:/Program Files (x86)/Steam";

/// Collection names written to Steam (prefixed with "SBO:" so we don't touch user collections).
pub fn collection_names() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("COMPLETED", "SBO: Completed"),
        ("IN_PROGRESS", "SBO: In Progress"),
        ("ENDLESS", "SBO: Endless"),
        ("NOT_A_GAME", "SBO: Not a Game"),
    ])
}

/// Check if Steam is currently running (Windows).
pub fn is_steam_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq steam.exe"])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
                stdout.contains("steam.exe")
            }
            Err(_) => false,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("pgrep")
            .arg("-x")
            .arg("steam")
            .output();
        matches!(output, Ok(out) if out.status.success())
    }
}

/// Steam account info found in userdata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SteamAccount {
    pub id: String,
    pub path: String,
}

/// List all Steam userdata account directories.
pub fn get_steam_accounts() -> Vec<SteamAccount> {
    let userdata = PathBuf::from(STEAM_BASE).join("userdata");
    if !userdata.exists() {
        return Vec::new();
    }

    let mut accounts = Vec::new();
    if let Ok(entries) = fs::read_dir(&userdata) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    accounts.push(SteamAccount {
                        id: name.to_string(),
                        path: entry.path().to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    accounts
}

/// Path to the cloud storage JSON for a given userdata account.
fn cloud_storage_path(userdata_path: &Path) -> PathBuf {
    userdata_path
        .join("config")
        .join("cloudstorage")
        .join("cloud-storage-namespace-1.json")
}

/// Load the cloud storage JSON that contains Steam collections.
pub fn load_steam_collections(userdata_path: &Path) -> Result<(Vec<Value>, PathBuf), String> {
    let path = cloud_storage_path(userdata_path);
    if !path.exists() {
        return Ok((Vec::new(), path));
    }

    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Could not read Steam collections file: {e}"))?;
    let parsed: Vec<Value> =
        serde_json::from_str(&data).map_err(|e| format!("Could not parse collections: {e}"))?;

    Ok((parsed, path))
}

/// Existing collection info parsed from cloud data.
struct ExistingCollection {
    key: String,
    id: String,
}

/// Extract user collections from cloud storage data.
fn get_existing_collections(cloud_data: &[Value]) -> HashMap<String, ExistingCollection> {
    let mut collections = HashMap::new();

    for entry in cloud_data {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };

        let key = match arr[0].as_str() {
            Some(k) if k.starts_with("user-collections.") => k,
            _ => continue,
        };

        let meta = match arr[1].as_object() {
            Some(m) => m,
            None => continue,
        };

        // Skip deleted entries
        if meta
            .get("is_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let value_str = match meta.get("value").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };

        let value: Value = match serde_json::from_str(value_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let name = match value.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let id = value
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        collections.insert(
            name,
            ExistingCollection {
                key: key.to_string(),
                id,
            },
        );
    }

    collections
}

/// Generate a random collection ID in Steam's format: uc-XXXXXXXXXXXX
fn generate_collection_id() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut rng = rand::rng();
    let random_part: String = (0..12)
        .map(|_| {
            let idx = rng.random_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect();
    format!("uc-{random_part}")
}

/// Find the highest version number in cloud data and return next.
fn get_next_version(cloud_data: &[Value]) -> u64 {
    let mut max_version: u64 = 0;
    for entry in cloud_data {
        if let Some(arr) = entry.as_array() {
            if arr.len() >= 2 {
                if let Some(v_str) = arr[1].get("version").and_then(|v| v.as_str()) {
                    if let Ok(v) = v_str.parse::<u64>() {
                        if v > max_version {
                            max_version = v;
                        }
                    }
                }
            }
        }
    }
    max_version + 1
}

/// Write classification results as Steam collections.
///
/// categories: { "COMPLETED": [appid1, appid2, ...], ... }
pub fn write_collections_to_steam(
    cloud_data: &mut Vec<Value>,
    cloud_path: &Path,
    categories: &HashMap<String, Vec<u64>>,
) -> Result<(), String> {
    let coll_names = collection_names();
    let existing = get_existing_collections(cloud_data);
    let mut version = get_next_version(cloud_data);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Time error: {e}"))?
        .as_secs();
    let mut modified_keys = Vec::new();

    for (cat_key, display_name) in &coll_names {
        let app_ids: Vec<Value> = categories
            .get(*cat_key)
            .map(|ids| ids.iter().map(|id| Value::from(*id)).collect())
            .unwrap_or_default();

        if let Some(coll) = existing.get(*display_name) {
            // Update existing collection
            let new_value = serde_json::json!({
                "id": coll.id,
                "name": display_name,
                "added": app_ids,
                "removed": [],
            });

            // Find and update the entry in cloud_data
            for entry in cloud_data.iter_mut() {
                if let Some(arr) = entry.as_array_mut() {
                    if arr.len() >= 2 {
                        if let Some(k) = arr[0].as_str() {
                            if k == coll.key {
                                if let Some(meta) = arr[1].as_object_mut() {
                                    meta.insert(
                                        "value".into(),
                                        Value::String(new_value.to_string()),
                                    );
                                    meta.insert(
                                        "timestamp".into(),
                                        Value::from(timestamp),
                                    );
                                    meta.insert(
                                        "version".into(),
                                        Value::String(version.to_string()),
                                    );
                                }
                                modified_keys.push(coll.key.clone());
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            // Create new collection
            let coll_id = generate_collection_id();
            let coll_key = format!("user-collections.{coll_id}");
            let new_value = serde_json::json!({
                "id": coll_id,
                "name": display_name,
                "added": app_ids,
                "removed": [],
            });

            cloud_data.push(serde_json::json!([
                coll_key.clone(),
                {
                    "key": coll_key.clone(),
                    "timestamp": timestamp,
                    "value": new_value.to_string(),
                    "version": version.to_string(),
                }
            ]));
            modified_keys.push(coll_key);
        }

        version += 1;
    }

    // Write the collection data
    let json_str = serde_json::to_string(cloud_data)
        .map_err(|e| format!("Failed to serialize collections: {e}"))?;
    fs::write(cloud_path, &json_str)
        .map_err(|e| format!("Could not write collections file: {e}"))?;

    // Update modified keys file so Steam syncs them
    let modified_path = cloud_path.with_file_name("cloud-storage-namespace-1.modified.json");
    let mut all_modified: Vec<String> = if modified_path.exists() {
        fs::read_to_string(&modified_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for key in &modified_keys {
        if !all_modified.contains(key) {
            all_modified.push(key.clone());
        }
    }
    let modified_json = serde_json::to_string(&all_modified)
        .map_err(|e| format!("Failed to serialize modified keys: {e}"))?;
    fs::write(&modified_path, &modified_json)
        .map_err(|e| format!("Could not write modified keys file: {e}"))?;

    // Update namespace version so Steam knows local state is newer
    let namespaces_path = cloud_path.with_file_name("cloud-storage-namespaces.json");
    if namespaces_path.exists() {
        if let Ok(data) = fs::read_to_string(&namespaces_path) {
            if let Ok(mut namespaces) = serde_json::from_str::<Vec<Value>>(&data) {
                for ns in &mut namespaces {
                    if let Some(arr) = ns.as_array_mut() {
                        if arr.len() >= 2 {
                            if arr[0].as_u64() == Some(1) {
                                arr[1] = Value::String(version.to_string());
                                break;
                            }
                        }
                    }
                }
                if let Ok(ns_json) = serde_json::to_string(&namespaces) {
                    let _ = fs::write(&namespaces_path, &ns_json);
                }
            }
        }
    }

    Ok(())
}
