import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-shell";
import {
  checkConfig,
  saveConfig,
  checkAiSetup,
  setupAi,
  getGpuStatus,
  setGpuEnabled,
  getModelTiers,
  setModelTier,
  deleteModelTier,
  onDownloadProgress,
  exportJson,
  SetupStatus,
  GpuStatus,
  DownloadProgress,
  ModelTiersResponse,
} from "../lib/commands";

interface Props {
  onClose: () => void;
  onConfigUpdated: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(1) + " " + units[i];
}

export default function SettingsPanel({ onClose, onConfigUpdated }: Props) {
  const [apiKey, setApiKey] = useState("");
  const [steamId, setSteamId] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // AI state
  const [aiSetup, setAiSetup] = useState<SetupStatus | null>(null);
  const [gpuStatus, setGpuStatus] = useState<GpuStatus | null>(null);
  const [tiers, setTiers] = useState<ModelTiersResponse | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] =
    useState<DownloadProgress | null>(null);

  useEffect(() => {
    loadExisting();
    const unlisten = onDownloadProgress((p) => setDownloadProgress(p));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function loadExisting() {
    const status = await checkConfig();
    if (status.steam_id) setSteamId(status.steam_id);

    try {
      const setup = await checkAiSetup();
      setAiSetup(setup);
    } catch {
      // not available
    }

    try {
      const gpu = await getGpuStatus();
      setGpuStatus(gpu);
    } catch {
      // not available
    }

    try {
      setTiers(await getModelTiers());
    } catch {
      // not available
    }
  }

  async function handleSave() {
    if (!apiKey.trim() && !steamId.trim()) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await saveConfig(apiKey.trim(), steamId.trim());
      setSaved(true);
      onConfigUpdated();
    } catch (e) {
      setError(String(e));
    }
    setSaving(false);
  }

  async function handleSetupAi(tier?: string) {
    setDownloading(true);
    setDownloadProgress(null);
    setError(null);
    try {
      await setupAi(tier);
      await refreshAiState();
    } catch (e) {
      setError(String(e));
    }
    setDownloading(false);
  }

  async function refreshAiState() {
    setAiSetup(await checkAiSetup());
    setGpuStatus(await getGpuStatus());
    setTiers(await getModelTiers());
  }

  async function handleActivateTier(tier: string) {
    setError(null);
    try {
      await setModelTier(tier);
      await refreshAiState();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDeleteTier(tier: string) {
    setError(null);
    try {
      await deleteModelTier(tier);
      await refreshAiState();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleGpuToggle(enabled: boolean) {
    try {
      await setGpuEnabled(enabled);
      const gpu = await getGpuStatus();
      setGpuStatus(gpu);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleExport() {
    try {
      const json = await exportJson();
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "steam-backlog-classifications.json";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setError(String(e));
    }
  }

  const aiReady = aiSetup?.modelReady && aiSetup?.serverReady;

  return (
    <div
      className="fixed inset-0 top-9 bg-black/60 flex items-center justify-center z-50 animate-fadeIn"
      onClick={onClose}
    >
      <div
        className="bg-steam-surface rounded-xl w-full max-w-md mx-4 p-6 border border-steam-border max-h-[85vh] overflow-y-auto animate-scaleIn"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-bold text-white mb-4">Settings</h2>

        <div className="space-y-4">
          {/* Steam config */}
          <div>
            <h3 className="text-sm font-medium text-steam-text mb-3 uppercase tracking-wide">
              Steam Configuration
            </h3>
            <div className="space-y-3">
              <div>
                <label className="block text-xs text-steam-text-dim mb-1">
                  Steam Web API Key
                </label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="Enter to update (hidden for security)"
                  className="w-full px-3 py-2 rounded-lg bg-steam-bg border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
                />
              </div>
              <div>
                <label className="block text-xs text-steam-text-dim mb-1">
                  Steam ID
                </label>
                <input
                  type="text"
                  value={steamId}
                  onChange={(e) => setSteamId(e.target.value)}
                  placeholder="17-digit Steam ID"
                  className="w-full px-3 py-2 rounded-lg bg-steam-bg border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
                />
              </div>
              <button
                onClick={handleSave}
                disabled={saving}
                className="py-2 px-4 rounded-lg bg-steam-surface-light text-sm text-white hover:bg-steam-blue transition-colors disabled:opacity-50"
              >
                {saving ? "Saving..." : "Update Config"}
              </button>
              {saved && (
                <div className="text-xs text-green-400">
                  Config updated. Re-sync to apply.
                </div>
              )}
            </div>
          </div>

          {/* AI Assistant */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-3 uppercase tracking-wide">
              AI Assistant
            </h3>
            <div className="space-y-3">
              <div className="text-xs text-steam-text-dim">
                Optional — adds conversational chat and a written taste profile.
                Recommendations, Discover, and wishlist scoring work without any
                model. Everything runs locally.
                {tiers?.vramMb
                  ? ` Detected ${(tiers.vramMb / 1024).toFixed(0)} GB VRAM.`
                  : " No discrete GPU detected — small models run on CPU."}
              </div>

              {/* No-AI row */}
              <div
                className={`flex items-center justify-between px-3 py-2 rounded-lg border ${
                  aiSetup && !aiSetup.activeTier
                    ? "border-steam-blue/60 bg-steam-surface-light"
                    : "border-steam-border"
                }`}
              >
                <div>
                  <div className="text-sm text-steam-text">No AI model</div>
                  <div className="text-xs text-steam-text-dim">
                    Taste engine only — instant, zero download
                  </div>
                </div>
                {aiSetup && !aiSetup.activeTier && (
                  <span className="text-xs text-steam-blue">active</span>
                )}
              </div>

              {/* Tier rows */}
              {tiers?.tiers.map((t) => (
                <div
                  key={t.id}
                  className={`flex items-center justify-between px-3 py-2 rounded-lg border ${
                    t.active
                      ? "border-steam-blue/60 bg-steam-surface-light"
                      : "border-steam-border"
                  }`}
                >
                  <div className="min-w-0">
                    <div className="text-sm text-steam-text flex items-center gap-2">
                      {t.label}
                      {t.recommended && (
                        <span className="px-1.5 py-0.5 rounded text-[10px] bg-steam-blue/20 text-steam-blue">
                          recommended for your hardware
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-steam-text-dim">
                      {formatBytes(t.sizeBytes)}
                      {t.installed ? " · installed" : " download"}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {t.active ? (
                      <span className="text-xs text-steam-blue">active</span>
                    ) : t.installed ? (
                      <>
                        <button
                          onClick={() => handleActivateTier(t.id)}
                          className="text-xs px-2 py-1 rounded bg-steam-blue text-white hover:bg-steam-blue-hover transition-colors"
                        >
                          Use
                        </button>
                        <button
                          onClick={() => handleDeleteTier(t.id)}
                          className="text-xs px-2 py-1 rounded text-steam-text-dim hover:text-red-400 transition-colors"
                        >
                          Delete
                        </button>
                      </>
                    ) : downloading ? (
                      <span className="text-xs text-steam-text-dim">…</span>
                    ) : (
                      <button
                        onClick={() => handleSetupAi(t.id)}
                        className="text-xs px-2 py-1 rounded bg-steam-surface-light text-steam-text hover:text-white transition-colors"
                      >
                        Download
                      </button>
                    )}
                  </div>
                </div>
              ))}

              {/* Download progress */}
              {downloading && downloadProgress && (
                <div>
                  <p className="text-sm text-steam-text mb-2">
                    {downloadProgress.stage}
                  </p>
                  <div className="w-full rounded-full h-2 bg-steam-bg">
                    <div
                      className="h-2 rounded-full bg-steam-blue transition-all duration-300"
                      style={{
                        width: `${Math.min(downloadProgress.percent, 100)}%`,
                      }}
                    />
                  </div>
                  <p className="text-xs text-steam-text-dim mt-1">
                    {formatBytes(downloadProgress.downloaded)}
                    {downloadProgress.total > 0 && (
                      <> / {formatBytes(downloadProgress.total)}</>
                    )}
                    {" — "}
                    {downloadProgress.percent.toFixed(0)}%
                  </p>
                </div>
              )}

              {/* GPU toggle (when a model is active) */}
              {aiReady && gpuStatus?.gpuDetected && gpuStatus?.cudaBuild && (
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={gpuStatus.usingGpu}
                    onChange={(e) => handleGpuToggle(e.target.checked)}
                    className="rounded border-steam-border"
                  />
                  <span className="text-sm text-steam-text">
                    Use GPU acceleration
                  </span>
                </label>
              )}
            </div>
          </div>

          {/* HLTB */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-2 uppercase tracking-wide">
              Completion Times
            </h3>
            <div className="text-xs text-steam-text-dim space-y-2">
              <p>
                Completion time estimates are provided by{" "}
                <a
                  href="https://howlongtobeat.com"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-steam-blue hover:underline"
                >
                  HowLongToBeat
                </a>
                . This is an unofficial integration using community-sourced data.
              </p>
              <p>
                HLTB data is fetched automatically after each sync and cached locally.
                Use the "Short games" filter in the library to find games that fit your schedule.
              </p>
            </div>
          </div>

          {/* Data */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-2 uppercase tracking-wide">
              Data
            </h3>
            <div className="text-xs text-steam-text-dim space-y-2">
              <p>
                Cache and config:{" "}
                <span className="font-mono text-steam-text">
                  %APPDATA%\Gamekeeper
                </span>
              </p>
              <p>
                Library cache refreshes every 24 hours. Use "Re-sync" to
                force.
              </p>
              <button
                onClick={handleExport}
                className="py-1.5 px-3 rounded-lg bg-steam-surface-light text-steam-text hover:text-white transition-colors"
              >
                Export classifications as JSON
              </button>
            </div>
          </div>

          {/* Help & Tips */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-3 uppercase tracking-wide">
              Help &amp; Tips
            </h3>
            <ul className="text-xs text-steam-text-dim space-y-2">
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">Click any game</strong> to view details, see why it was classified, or change its category</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">Sidebar categories</strong> filter your library — counts update as you override games</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">"What should I play?"</strong> opens the AI assistant for personalized recommendations</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">"Write to Steam"</strong> pushes your collections into Steam so they appear across devices</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">"Re-sync library"</strong> refreshes your games from Steam — useful after buying new games</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                <span><strong className="text-steam-text">AI assistant</strong> — download the model above to enable game recommendations and classification second opinions</span>
              </li>
            </ul>
          </div>

          {/* System Requirements */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-3 uppercase tracking-wide">
              System Requirements
            </h3>
            <div className="space-y-3">
              <div>
                <div className="text-xs font-medium text-steam-text mb-1">Minimum (without AI)</div>
                <ul className="text-xs text-steam-text-dim space-y-0.5">
                  <li>OS: Windows 10 64-bit</li>
                  <li>RAM: 4 GB</li>
                  <li>Storage: 100 MB</li>
                  <li>Network: Internet for Steam API sync</li>
                </ul>
              </div>
              <div>
                <div className="text-xs font-medium text-steam-text mb-1">Optional AI chat (by model tier)</div>
                <ul className="text-xs text-steam-text-dim space-y-0.5">
                  <li>Standard (2.7 GB): 4 GB VRAM, or CPU with 12 GB RAM</li>
                  <li>Plus (5 GB): 8 GB VRAM GPU</li>
                  <li>Max (15.7 GB): 20+ GB VRAM GPU</li>
                  <li>Taste engine, Discover &amp; wishlist scoring need no AI model at all</li>
                </ul>
              </div>
            </div>
          </div>

          {/* About */}
          <div className="pt-3 border-t border-steam-border">
            <h3 className="text-sm font-medium text-steam-text mb-2 uppercase tracking-wide">
              About
            </h3>
            <div className="text-xs text-steam-text-dim space-y-2">
              <p>Gamekeeper v3.0</p>
              <p>Created by LordVelm</p>
              <p>
                Rule-based classification — no cloud AI required.
                Optional local AI for recommendations.
              </p>
              <div className="flex gap-2">
                <button
                  onClick={() => open("https://buymeacoffee.com/lordvelm")}
                  className="py-1.5 px-3 rounded-lg bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30 transition-colors font-medium"
                >
                  Support the project
                </button>
                <button
                  onClick={() => open("https://github.com/LordVelm/steam-backlog-organizer/issues")}
                  className="py-1.5 px-3 rounded-lg bg-steam-surface-light text-steam-text-dim hover:text-white transition-colors"
                >
                  Report a bug
                </button>
              </div>
            </div>
          </div>

          {error && (
            <div className="p-3 rounded-lg bg-red-900/30 border border-red-700 text-red-300 text-sm">
              {error}
            </div>
          )}
        </div>

        <button
          onClick={onClose}
          className="w-full mt-5 py-2 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
        >
          Close
        </button>
      </div>
    </div>
  );
}
