import { useState, useEffect, useMemo } from "react";
import {
  Classification,
  CategoryKey,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
  STEAM_HEADER_URL,
  setOverride,
} from "../lib/commands";
import GameDetail from "./GameDetail";

const TIPS_DISMISSED_KEY = "steam-backlog-tips-dismissed";

interface Props {
  games: Classification[];
  onOverrideChange: () => void;
}

export default function GameGrid({ games, onOverrideChange }: Props) {
  const [search, setSearch] = useState("");
  const [selectedGame, setSelectedGame] = useState<Classification | null>(null);
  const [showTips, setShowTips] = useState(false);

  useEffect(() => {
    const dismissed = localStorage.getItem(TIPS_DISMISSED_KEY);
    if (!dismissed) setShowTips(true);
  }, []);

  const sorted = useMemo(() => {
    const filtered = search.trim()
      ? games.filter((g) =>
          g.name.toLowerCase().includes(search.toLowerCase())
        )
      : games;
    return [...filtered].sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase())
    );
  }, [games, search]);

  return (
    <div className="flex flex-col h-full">
      {/* Search bar */}
      <div className="p-4 border-b border-steam-border">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search games..."
          className="w-full px-3 py-2 rounded-lg bg-steam-surface border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
        />
        <div className="mt-2 text-xs text-steam-text-dim">
          {sorted.length} game{sorted.length !== 1 ? "s" : ""}
        </div>
      </div>

      {/* Welcome tips */}
      {showTips && (
        <div className="mx-4 mt-3 p-4 rounded-lg bg-steam-blue/10 border border-steam-blue/25 animate-fadeIn">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-white mb-2">Welcome to Steam Backlog Organizer</h3>
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
                  <span><strong className="text-steam-text">Settings</strong> lets you update your Steam config, download the AI model, export data, and more</span>
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
              onClick={() => setSelectedGame(game)}
            />
          ))}
        </div>

        {sorted.length === 0 && (
          <div className="flex items-center justify-center h-48 text-steam-text-dim">
            {search ? "No games match your search." : "No games in this category."}
          </div>
        )}
      </div>

      {/* Detail drawer */}
      {selectedGame && (
        <GameDetail
          game={selectedGame}
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
  onClick,
}: {
  game: Classification;
  onClick: () => void;
}) {
  const [imgError, setImgError] = useState(false);
  const color = CATEGORY_COLORS[game.category];

  return (
    <button
      onClick={onClick}
      className="game-card-hover group relative rounded-lg overflow-hidden bg-steam-surface border border-steam-border hover:border-steam-blue transition-all text-left"
    >
      {/* Game image */}
      <div className="aspect-[460/215] bg-steam-surface-light">
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
