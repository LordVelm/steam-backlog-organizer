import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-shell";
import {
  checkConfig,
  saveConfig,
  checkAiSetup,
  setupAi,
  getGpuStatus,
  setGpuEnabled,
  onDownloadProgress,
  exportJson,
  SetupStatus,
  GpuStatus,
  DownloadProgress,
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

  async function handleSetupAi() {
    setDownloading(true);
    setDownloadProgress(null);
    setError(null);
    try {
      await setupAi();
      const setup = await checkAiSetup();
      setAiSetup(setup);
      const gpu = await getGpuStatus();
      setGpuStatus(gpu);
    } catch (e) {
      setError(String(e));
    }
    setDownloading(false);
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
              {aiReady ? (
                <>
                  {/* AI is ready */}
                  <div className="flex items-center gap-2 text-xs">
                    <span className="w-2 h-2 rounded-full bg-green-400" />
                    <span className="text-steam-text-dim">
                      AI ready
                      {gpuStatus?.usingGpu
                        ? " (GPU accelerated)"
                        : " (CPU mode)"}
                    </span>
                  </div>

                  {/* GPU toggle */}
                  {gpuStatus?.gpuDetected && gpuStatus?.cudaBuild ? (
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
                  ) : gpuStatus && !gpuStatus.gpuDetected ? (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="w-2 h-2 rounded-full bg-yellow-400" />
                      <span className="text-steam-text-dim">
                        No discrete GPU detected — running on CPU (slower but fully functional)
                      </span>
                    </div>
                  ) : null}

                  <div className="text-xs text-steam-text-dim">
                    Powers game recommendations and classification second
                    opinions. Everything runs locally on your machine.
                    {gpuStatus?.usingGpu
                      ? " Using your GPU for faster inference."
                      : gpuStatus?.gpuDetected
                        ? " GPU available but currently using CPU."
                        : ""}
                  </div>
                </>
              ) : downloading ? (
                <>
                  {/* Download in progress */}
                  {downloadProgress ? (
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
                  ) : (
                    <p className="text-sm text-steam-text-dim animate-pulse">
                      Getting things ready...
                    </p>
                  )}
                </>
              ) : (
                <>
                  {/* AI not set up */}
                  <div className="text-xs text-steam-text-dim mb-2">
                    Download the AI model (~3.5 GB) to enable game
                    recommendations and classification assistance. One-time
                    download — everything runs locally.
                  </div>
                  <button
                    onClick={handleSetupAi}
                    className="py-2 px-4 rounded-lg bg-steam-blue text-sm text-white font-medium hover:bg-steam-blue-hover transition-colors"
                  >
                    Download AI Model
                  </button>
                </>
              )}
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
                <div className="text-xs font-medium text-steam-text mb-1">Recommended (with AI assistant)</div>
                <ul className="text-xs text-steam-text-dim space-y-0.5">
                  <li>OS: Windows 10/11 64-bit</li>
                  <li>RAM: 8 GB (16 GB for best performance)</li>
                  <li>Storage: 5 GB free (AI model is ~3.5 GB)</li>
                  <li>GPU: NVIDIA GPU with 4+ GB VRAM (optional, uses CUDA)</li>
                  <li>CPU fallback: Works without GPU, responses are slower</li>
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
