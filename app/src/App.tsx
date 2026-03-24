import { useEffect, useRef, useState } from "react";
import {
  checkConfig,
  fetchLibrary,
  fetchStoreDetails,
  classifyGames,
  getClassifications,
  fetchHltbData,
  getHltbData,
  onSyncProgress,
  cancelSync,
  Classification,
  CategoryKey,
  HltbData,
  ConfigStatus,
  SyncProgress as SyncProgressEvent,
} from "./lib/commands";
import TitleBar from "./components/TitleBar";
import SetupScreen from "./components/SetupScreen";
import GameGrid from "./components/GameGrid";
import Sidebar from "./components/Sidebar";
import WriteToSteam from "./components/WriteToSteam";
import SettingsPanel from "./components/SettingsPanel";
import ChatPanel from "./components/ChatPanel";

type AppPhase = "loading" | "setup" | "syncing" | "ready";

interface SyncState {
  step: string;
  detail: string;
  current: number;
  total: number;
  eta: string;
}

function formatEta(seconds: number): string {
  if (seconds < 60) return `~${Math.ceil(seconds)}s remaining`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.ceil(seconds % 60);
  if (mins < 2) return `~${mins}m ${secs}s remaining`;
  return `~${mins}m remaining`;
}

export default function App() {
  const [phase, setPhase] = useState<AppPhase>("loading");
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [classifications, setClassifications] = useState<Classification[]>([]);
  const [activeCategory, setActiveCategory] = useState<CategoryKey | "ALL">("ALL");
  const [hltbData, setHltbData] = useState<Record<string, HltbData>>({});
  const [hltbLoading, setHltbLoading] = useState(false);
  const [syncState, setSyncState] = useState<SyncState>({
    step: "",
    detail: "",
    current: 0,
    total: 0,
    eta: "",
  });
  const [error, setError] = useState<string | null>(null);
  const [showWriteToSteam, setShowWriteToSteam] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showChat, setShowChat] = useState(false);

  // Track when each sync step started for ETA calculation
  const stepStartTime = useRef<number>(0);
  const lastStep = useRef<string>("");

  useEffect(() => {
    initialize();

    const unlisten = onSyncProgress((p: SyncProgressEvent) => {
      // Reset timer when step changes
      if (p.step !== lastStep.current) {
        stepStartTime.current = Date.now();
        lastStep.current = p.step;
      }

      // Calculate ETA
      let eta = "";
      const elapsed = (Date.now() - stepStartTime.current) / 1000;
      if (p.current > 1 && p.total > 0 && elapsed > 2) {
        const rate = (p.current - 1) / elapsed; // items per second
        const remaining = p.total - p.current;
        if (rate > 0) {
          eta = formatEta(remaining / rate);
        }
      }

      setSyncState({
        step: p.step,
        detail: `${p.step} (${p.current} / ${p.total})`,
        current: p.current,
        total: p.total,
        eta,
      });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  /** Load cached HLTB data then kick off a background fetch for any missing games. */
  function startHltbFetch() {
    setHltbLoading(true);
    getHltbData()
      .then((cached) => setHltbData(cached))
      .catch(() => {});

    fetchHltbData()
      .then(() => getHltbData())
      .then((updated) => {
        setHltbData(updated);
        setHltbLoading(false);
      })
      .catch(() => setHltbLoading(false));
  }

  async function initialize() {
    try {
      const status = await checkConfig();
      setConfigStatus(status);
      if (!status.configured) {
        setPhase("setup");
        return;
      }

      // Load any existing classifications
      const existing = await getClassifications();
      if (existing.length > 0) {
        setClassifications(existing);
        setPhase("ready");
        startHltbFetch();
      } else {
        setPhase("setup");
      }
    } catch (e) {
      setError(String(e));
      setPhase("setup");
    }
  }

  async function handleSetupComplete() {
    setPhase("syncing");
    setError(null);
    try {
      setSyncState({ step: "Fetching library", detail: "Getting your games from Steam...", current: 0, total: 0, eta: "" });
      await fetchLibrary();

      setSyncState({ step: "Fetching details", detail: "Loading store data for each game...", current: 0, total: 0, eta: "" });
      await fetchStoreDetails();

      setSyncState({ step: "Classifying", detail: "Sorting your games into categories...", current: 0, total: 0, eta: "" });
      const results = await classifyGames();
      setClassifications(results);

      setPhase("ready");
      startHltbFetch();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        // User cancelled — go back to ready if we have data, otherwise setup
        const existing = await getClassifications().catch(() => []);
        if (existing.length > 0) {
          setClassifications(existing);
          setPhase("ready");
        } else {
          setPhase("setup");
        }
      } else {
        setError(msg);
        setPhase("setup");
      }
    }
  }

  async function handleResync() {
    setPhase("syncing");
    setError(null);
    try {
      setSyncState({ step: "Fetching library", detail: "Refreshing your games...", current: 0, total: 0, eta: "" });
      await fetchLibrary();

      setSyncState({ step: "Fetching details", detail: "Updating store data...", current: 0, total: 0, eta: "" });
      await fetchStoreDetails();

      setSyncState({ step: "Classifying", detail: "Re-sorting your games...", current: 0, total: 0, eta: "" });
      const results = await classifyGames();
      setClassifications(results);

      setPhase("ready");
      startHltbFetch();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        setPhase("ready");
      } else {
        setError(msg);
        setPhase("ready");
      }
    }
  }

  async function handleCancelSync() {
    await cancelSync();
  }

  const filteredGames =
    activeCategory === "ALL"
      ? classifications
      : classifications.filter((c) => c.category === activeCategory);

  const counts = {
    ALL: classifications.length,
    COMPLETED: classifications.filter((c) => c.category === "COMPLETED").length,
    IN_PROGRESS: classifications.filter((c) => c.category === "IN_PROGRESS").length,
    ENDLESS: classifications.filter((c) => c.category === "ENDLESS").length,
    NOT_A_GAME: classifications.filter((c) => c.category === "NOT_A_GAME").length,
  };

  const syncPercent = syncState.total > 0
    ? Math.round((syncState.current / syncState.total) * 100)
    : 0;

  if (phase === "loading") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex items-center justify-center flex-1">
          <div className="text-steam-text-dim text-lg">Loading...</div>
        </div>
      </div>
    );
  }

  if (phase === "setup") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex-1 overflow-auto">
          <SetupScreen
            onComplete={handleSetupComplete}
            error={error}
            hasExistingConfig={configStatus?.configured ?? false}
          />
        </div>
      </div>
    );
  }

  if (phase === "syncing") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex flex-col items-center justify-center flex-1 gap-6">
          <div className="w-12 h-12 border-4 border-steam-blue border-t-transparent rounded-full animate-spin" />
          <div className="text-center w-80">
            <div className="text-xl font-semibold text-white mb-2">
              {syncState.step}
            </div>
            <div className="text-steam-text-dim text-sm mb-4">
              {syncState.detail}
            </div>
            {syncState.total > 0 && (
              <div>
                <div className="w-full rounded-full h-2 bg-steam-surface-light">
                  <div
                    className="h-2 rounded-full bg-steam-blue transition-all duration-300"
                    style={{ width: `${syncPercent}%` }}
                  />
                </div>
                <div className="flex justify-between text-xs text-steam-text-dim mt-2">
                  <span>{syncState.current} / {syncState.total} — {syncPercent}%</span>
                  {syncState.eta && <span>{syncState.eta}</span>}
                </div>
              </div>
            )}
            <button
              onClick={handleCancelSync}
              className="mt-6 py-2 px-6 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light border border-steam-border transition-colors"
            >
              Stop syncing
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen">
      <TitleBar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          activeCategory={activeCategory}
          onCategoryChange={setActiveCategory}
          counts={counts}
          onResync={handleResync}
          onWriteToSteam={() => setShowWriteToSteam(true)}
          onSettings={() => setShowSettings(true)}
          onChat={() => setShowChat(true)}
        />
        <main className="flex-1 overflow-hidden">
          <GameGrid
            games={filteredGames}
            hltbData={hltbData}
            hltbLoading={hltbLoading}
            onOverrideChange={async () => {
              const results = await classifyGames();
              setClassifications(results);
            }}
          />
        </main>
      </div>
      {showWriteToSteam && (
        <WriteToSteam
          onClose={() => setShowWriteToSteam(false)}
          totalGames={classifications.length}
        />
      )}
      {showSettings && (
        <SettingsPanel
          onClose={() => setShowSettings(false)}
          onConfigUpdated={() => {}}
        />
      )}
      {showChat && <ChatPanel onClose={() => setShowChat(false)} />}
    </div>
  );
}
