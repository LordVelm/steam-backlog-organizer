import { useState, useEffect, useMemo } from "react";
import {
  Classification,
  CategoryKey,
  HltbData,
  LengthBucket,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
  LENGTH_BUCKET_COLORS,
  getLengthBucket,
  formatHours,
  STEAM_HEADER_URL,
  setOverride,
} from "../lib/commands";
import GameDetail from "./GameDetail";

const TIPS_DISMISSED_KEY = "steam-backlog-tips-dismissed";

const LENGTH_FILTERS: { key: LengthBucket | "ALL"; label: string }[] = [
  { key: "ALL", label: "All" },
  { key: "Quick", label: "< 2h" },
  { key: "Short", label: "2–8h" },
  { key: "Medium", label: "8–20h" },
  { key: "Long", label: "20–50h" },
  { key: "VeryLong", label: "50h+" },
  { key: "Unknown", label: "?" },
];

interface Props {
  games: Classification[];
  hltbData: Record<string, HltbData>;
  hltbLoading: boolean;
  onOverrideChange: () => void;
}

export default function GameGrid({ games, hltbData, hltbLoading, onOverrideChange }: Props) {
  const [search, setSearch] = useState("");
  const [lengthFilter, setLengthFilter] = useState<LengthBucket | "ALL">("ALL");
  const [selectedGame, setSelectedGame] = useState<Classification | null>(null);
  const [showTips, setShowTips] = useState(false);

  useEffect(() => {
    const dismissed = localStorage.getItem(TIPS_DISMISSED_KEY);
    if (!dismissed) setShowTips(true);
  }, []);

  const sorted = useMemo(() => {
    let filtered = search.trim()
      ? games.filter((g) => g.name.toLowerCase().includes(search.toLowerCase()))
      : games;

    if (lengthFilter !== "ALL") {
      filtered = filtered.filter((g) => {
        const entry = hltbData[g.appid.toString()];
        const bucket = getLengthBucket(entry?.main_story ?? null);
        return bucket === lengthFilter;
      });
    }

    return [...filtered].sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase())
    );
  }, [games, search, lengthFilter, hltbData]);

  return (
    <div className="flex flex-col h-full">
      {/* Search + length filter bar */}
      <div className="p-4 border-b border-steam-border space-y-3">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search games..."
          className="w-full px-3 py-2 rounded-lg bg-steam-surface border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
        />

        {/* Length filter chips */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-xs text-steam-text-dim shrink-0 mr-0.5">Length:</span>
          {LENGTH_FILTERS.map(({ key, label }) => {
            const active = lengthFilter === key;
            const color = key !== "ALL" ? LENGTH_BUCKET_COLORS[key as LengthBucket] : undefined;
            return (
              <button
                key={key}
                onClick={() => setLengthFilter(key)}
                className={`px-2.5 py-0.5 rounded-full text-xs font-medium transition-colors border ${
                  active
                    ? "text-steam-bg border-transparent"
                    : "text-steam-text-dim border-steam-border hover:text-white hover:border-steam-text-dim"
                }`}
                style={active && color ? { backgroundColor: color, borderColor: color } : active ? { backgroundColor: "#66c0f4", borderColor: "#66c0f4" } : {}}
              >
                {label}
              </button>
            );
          })}
          {hltbLoading && (
            <span className="text-xs text-steam-text-dim animate-pulse ml-1">
              Loading lengths…
            </span>
          )}
        </div>

        <div className="text-xs text-steam-text-dim">
          {sorted.length} game{sorted.length !== 1 ? "s" : ""}
        </div>
      </div>

      {/* Welcome tips */}
      {showTips && (
        <div className="mx-4 mt-3 p-4 rounded-lg bg-steam-blue/10 border border-steam-blue/25 animate-fadeIn">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-white mb-2">Welcome to Gamekeeper</h3>
              <ul className="text-xs text-steam-text-dim space-y-1.5">
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
                  <span><strong className="text-steam-text">Length filter</strong> shows only games that fit your available time — powered by HowLongToBeat</span>
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
              </ul>
            </div>
            <button
              onClick={() => {
                setShowTips(false);
                localStorage.setItem(TIPS_DISMISSED_KEY, "1");
              }}
              className="text-steam-text-dim hover:text-white transition-colors shrink-0 p-1"
              title="Dismiss"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M1 1l12 12M13 1L1 13" />
              </svg>
            </button>
          </div>
        </div>
      )}

      {/* Grid */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
          {sorted.map((game) => (
            <GameCard
              key={game.appid}
              game={game}
              hltbEntry={hltbData[game.appid.toString()] ?? null}
              onClick={() => setSelectedGame(game)}
            />
          ))}
        </div>

        {sorted.length === 0 && (
          <div className="flex items-center justify-center h-48 text-steam-text-dim">
            {search || lengthFilter !== "ALL"
              ? "No games match your filters."
              : "No games in this category."}
          </div>
        )}
      </div>

      {/* Detail drawer */}
      {selectedGame && (
        <GameDetail
          game={selectedGame}
          hltbEntry={hltbData[selectedGame.appid.toString()] ?? null}
          onClose={() => setSelectedGame(null)}
          onOverride={async (appid, category) => {
            await setOverride(String(appid), category);
            onOverrideChange();
            setSelectedGame(null);
          }}
        />
      )}
    </div>
  );
}

function GameCard({
  game,
  hltbEntry,
  onClick,
}: {
  game: Classification;
  hltbEntry: HltbData | null;
  onClick: () => void;
}) {
  const [imgError, setImgError] = useState(false);
  const color = CATEGORY_COLORS[game.category];
  const bucket = getLengthBucket(hltbEntry?.main_story ?? null);
  const badgeColor = LENGTH_BUCKET_COLORS[bucket];
  const timeLabel = hltbEntry?.main_story != null
    ? `~${formatHours(hltbEntry.main_story)}`
    : null;

  return (
    <button
      onClick={onClick}
      className="game-card-hover group relative rounded-lg overflow-hidden bg-steam-surface border border-steam-border hover:border-steam-blue transition-all text-left"
    >
      {/* Game image */}
      <div className="aspect-[460/215] bg-steam-surface-light relative">
        {!imgError ? (
          <img
            src={STEAM_HEADER_URL(game.appid)}
            alt={game.name}
            className="w-full h-full object-cover"
            loading="lazy"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-steam-text-dim text-xs p-2 text-center">
            {game.name}
          </div>
        )}

        {/* HLTB time badge */}
        {timeLabel && (
          <span
            className="absolute top-1.5 right-1.5 px-1.5 py-0.5 rounded text-xs font-semibold text-steam-bg leading-none"
            style={{ backgroundColor: badgeColor }}
          >
            {timeLabel}
          </span>
        )}
      </div>

      {/* Info */}
      <div className="p-2">
        <div className="text-sm text-white truncate font-medium">
          {game.name}
        </div>
        <div className="flex items-center gap-1.5 mt-1">
          <span
            className="w-2 h-2 rounded-full shrink-0"
            style={{ backgroundColor: color }}
          />
          <span className="text-xs text-steam-text-dim truncate">
            {CATEGORY_LABELS[game.category]}
          </span>
        </div>
      </div>
    </button>
  );
}
