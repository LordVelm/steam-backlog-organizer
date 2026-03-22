import { useState } from "react";
import { saveConfig, checkConfig } from "../lib/commands";

interface Props {
  onComplete: () => void;
  error: string | null;
  hasExistingConfig: boolean;
}

export default function SetupScreen({ onComplete, error, hasExistingConfig }: Props) {
  const [apiKey, setApiKey] = useState("");
  const [steamId, setSteamId] = useState("");
  const [saving, setSaving] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!apiKey.trim() || !steamId.trim()) {
      setLocalError("Both fields are required.");
      return;
    }

    setSaving(true);
    setLocalError(null);
    try {
      await saveConfig(apiKey.trim(), steamId.trim());
      onComplete();
    } catch (err) {
      setLocalError(String(err));
      setSaving(false);
    }
  }

  async function handleUseSaved() {
    onComplete();
  }

  return (
    <div className="flex items-center justify-center h-screen">
      <div className="w-full max-w-md p-8">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-white mb-2">
            Gamekeeper
          </h1>
          <p className="text-steam-text-dim">
            Automatically organize your Steam library into collections.
          </p>
        </div>

        {hasExistingConfig && (
          <button
            onClick={handleUseSaved}
            className="w-full mb-6 py-3 px-4 rounded-lg bg-steam-blue text-white font-semibold hover:bg-steam-blue-hover transition-colors"
          >
            Sync with saved config
          </button>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-steam-text mb-1">
              Steam Web API Key
            </label>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="Your Steam API key"
              className="w-full px-3 py-2 rounded-lg bg-steam-surface border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none"
            />
            <p className="text-xs text-steam-text-dim mt-1">
              Get one free at{" "}
              <span className="text-steam-blue">
                steamcommunity.com/dev/apikey
              </span>
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-steam-text mb-1">
              Steam ID
            </label>
            <input
              type="text"
              value={steamId}
              onChange={(e) => setSteamId(e.target.value)}
              placeholder="Your 17-digit Steam ID or vanity URL"
              className="w-full px-3 py-2 rounded-lg bg-steam-surface border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none"
            />
          </div>

          {(error || localError) && (
            <div className="p-3 rounded-lg bg-red-900/30 border border-red-700 text-red-300 text-sm">
              {localError || error}
            </div>
          )}

          <button
            type="submit"
            disabled={saving}
            className="w-full py-3 px-4 rounded-lg bg-steam-surface-light text-white font-semibold hover:bg-steam-blue transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : hasExistingConfig ? "Update & Sync" : "Save & Sync"}
          </button>
        </form>
      </div>
    </div>
  );
}
