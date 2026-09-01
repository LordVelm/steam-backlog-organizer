//! Hardware probing for AI model tier recommendation.
//! Chain: nvidia-smi (dedicated VRAM, NVIDIA only) → DXGI adapter descriptor
//! (vendor-agnostic) → system RAM ceiling for the CPU path.

use crate::llm::{ModelTier, MODEL_TIERS};
use serde::Serialize;
use std::process::{Command, Stdio};

/// Dedicated VRAM in MB via nvidia-smi. Probes PATH plus the standard install
/// locations (drivers don't always add it to PATH).
pub fn query_vram_nvidia_mb() -> Option<u64> {
    let candidates: &[&str] = &[
        "nvidia-smi",
        r"C:\Windows\System32\nvidia-smi.exe",
        r"C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe",
    ];
    for exe in candidates {
        let mut cmd = Command::new(exe);
        cmd.args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                // One line per GPU; take the largest
                let best = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|l| l.trim().parse::<u64>().ok())
                    .max();
                if best.is_some() {
                    return best;
                }
            }
        }
    }
    None
}

/// Dedicated VRAM in MB via DXGI — vendor-agnostic (AMD/Intel/NVIDIA).
/// Skips Microsoft Basic Render (vendor 0x1414).
#[cfg(windows)]
pub fn query_vram_dxgi_mb() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut best: Option<u64> = None;
    for i in 0.. {
        let adapter = match unsafe { factory.EnumAdapters1(i) } {
            Ok(a) => a,
            Err(_) => break,
        };
        if let Ok(desc) = unsafe { adapter.GetDesc1() } {
            if desc.VendorId == 0x1414 {
                continue; // Microsoft Basic Render Driver
            }
            let mb = (desc.DedicatedVideoMemory as u64) / (1024 * 1024);
            if mb > best.unwrap_or(0) {
                best = Some(mb);
            }
        }
    }
    best.filter(|&mb| mb >= 512) // ignore tiny iGPU carve-outs
}

#[cfg(not(windows))]
pub fn query_vram_dxgi_mb() -> Option<u64> {
    None
}

/// Total physical RAM in MB.
#[cfg(windows)]
pub fn query_ram_mb() -> u64 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok() {
        status.ullTotalPhys / (1024 * 1024)
    } else {
        0
    }
}

#[cfg(not(windows))]
pub fn query_ram_mb() -> u64 {
    0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub vram_mb: Option<u64>,
    pub ram_mb: u64,
    pub recommended_tier: Option<String>,
}

pub fn probe() -> HardwareInfo {
    let vram_mb = query_vram_nvidia_mb().or_else(query_vram_dxgi_mb);
    let ram_mb = query_ram_mb();
    HardwareInfo {
        vram_mb,
        ram_mb,
        recommended_tier: recommend_tier(vram_mb, ram_mb).map(|t| t.id.to_string()),
    }
}

/// Largest tier that fits: by dedicated VRAM when a discrete GPU is present,
/// else RAM-gated CPU inference (small tiers only — big models on CPU are
/// unusably slow regardless of RAM).
pub fn recommend_tier(vram_mb: Option<u64>, ram_mb: u64) -> Option<&'static ModelTier> {
    if let Some(vram) = vram_mb {
        if let Some(tier) = MODEL_TIERS
            .iter()
            .rev()
            .find(|t| vram >= t.min_vram_mb)
        {
            return Some(tier);
        }
    }
    // CPU path: only ever recommend the smallest tier, and only with headroom
    MODEL_TIERS
        .first()
        .filter(|t| ram_mb >= t.min_ram_mb_cpu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_tier_by_vram() {
        assert_eq!(recommend_tier(Some(32_768), 32_768).unwrap().id, "qwen2.5-14b-q8");
        assert_eq!(recommend_tier(Some(24_576), 32_768).unwrap().id, "qwen2.5-14b-q8");
        assert_eq!(recommend_tier(Some(12_288), 32_768).unwrap().id, "qwen3-8b-q4");
        assert_eq!(recommend_tier(Some(8_192), 32_768).unwrap().id, "qwen3-8b-q4");
        assert_eq!(recommend_tier(Some(6_144), 32_768).unwrap().id, "qwen3.5-4b-q4");
        assert_eq!(recommend_tier(Some(4_096), 32_768).unwrap().id, "qwen3.5-4b-q4");
    }

    #[test]
    fn recommend_tier_cpu_fallback() {
        // Tiny VRAM but plenty of RAM → smallest tier on CPU
        assert_eq!(recommend_tier(Some(2_048), 32_768).unwrap().id, "qwen3.5-4b-q4");
        assert_eq!(recommend_tier(None, 16_384).unwrap().id, "qwen3.5-4b-q4");
        // Not enough RAM either → no recommendation ("taste engine only")
        assert!(recommend_tier(None, 8_192).is_none());
        assert!(recommend_tier(Some(1_024), 8_192).is_none());
    }

    #[test]
    fn probe_runs_on_this_machine() {
        let hw = probe();
        assert!(hw.ram_mb > 1_000, "RAM probe returned {}", hw.ram_mb);
        // On Kareem's 5090 box this should detect VRAM, but don't hard-require
        // it — CI/other machines may have no GPU.
    }
}
