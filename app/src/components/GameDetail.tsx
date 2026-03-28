import { useState, useEffect } from "react";
import {
  Classification,
  CategoryKey,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
  STEAM_HEADER_URL,
  HltbEntry,
  getAmbiguitySuggestion,
  checkAiSetup,
  AmbiguityResponse,
} from "../lib/commands";

interface Props {
  game: Classification;
  hltb?: HltbEntry;
  playtimeHours?: number;
  onClose: () => void;
  onOverride: (appid: number, category: CategoryKey) => void;
}

const CATEGORIES: CategoryKey[] = [
  "COMPLETED",
  "IN_PROGRESS",
  "ENDLESS",
  "NOT_A_GAME",
];

export default function GameDetail({ game, hltb, playtimeHours = 0, onClose, onOverride }: Props) {
  const [aiSuggestion, setAiSuggestion] = useState<AmbiguityResponse | null>(
    null
  );
  const [aiLoading, setAiLoading] = useState(false);
  const [aiReady, setAiReady] = useState<boolean | null>(null);

  useEffect(() => {
    checkAiSetup().then((s) => setAiReady(s.modelReady && s.serverReady)).catch(() => setAiReady(false));
  }, []);

  async function handleAskAI() {
    setAiLoading(true);
    try {
      const result = await getAmbiguitySuggestion(game.appid);
      setAiSuggestion(result);
    } catch (e) {
      setAiSuggestion({
        suggested_category: game.category,
        rationale: `Error: ${e}`,
        used_llm: false,
      });
    }
    setAiLoading(false);
  }

  return (
    <div
      className="fixed inset-0 top-9 bg-black/60 flex items-center justify-center z-50 animate-fadeIn"
      onClick={onClose}
    >
      <div
        className="bg-steam-surface rounded-xl w-full max-w-lg mx-4 overflow-hidden border border-steam-border animate-scaleIn"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header image */}
        <img
          src={STEAM_HEADER_URL(game.appid)}
          alt={game.name}
          className="w-full aspect-[460/215] object-cover"
        />

        <div className="p-5">
          <h2 className="text-xl font-bold text-white mb-1">{game.name}</h2>

          {/* Current category */}
          <div className="flex items-center gap-2 mb-4">
            <span
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: CATEGORY_COLORS[game.category] }}
            />
            <span className="text-sm text-steam-text">
              {CATEGORY_LABELS[game.category]}
            </span>
            <span className="text-xs text-steam-text-dim px-2 py-0.5 rounded bg-steam-surface-light">
              {game.confidence}
            </span>
          </div>

          {/* HLTB completion times */}
          {hltb && hltb.match_status === "matched" && hltb.main_story_hours != null && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">~Time Left</div>
                  <div className="text-sm font-semibold text-white">
                    {(() => {
                      const remaining = Math.round((hltb.main_story_hours! - playtimeHours) * 10) / 10;
                      return remaining > 0
                        ? `${remaining}h`
                        : <span className="text-[#4CAF50]">Likely complete</span>;
                    })()}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Main Story</div>
                  <div className="text-xs text-steam-text-dim">{hltb.main_story_hours}h</div>
                </div>
                {hltb.main_extra_hours != null && (
                  <div>
                    <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Main + Extra</div>
                    <div className="text-xs text-steam-text-dim">{hltb.main_extra_hours}h</div>
                  </div>
                )}
                {hltb.completionist_hours != null && (
                  <div>
                    <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Completionist</div>
                    <div className="text-xs text-steam-text-dim">{hltb.completionist_hours}h</div>
                  </div>
                )}
              </div>
              <div className="mt-2 text-[10px] text-steam-text-dim">
                Data from{" "}
                <a
                  href="https://howlongtobeat.com"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-steam-blue hover:underline"
                >
                  HowLongToBeat
                </a>
                {" "}&middot; community-sourced estimates
              </div>
            </div>
          )}
          {hltb && hltb.match_status === "no_match" && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg text-xs text-steam-text-dim">
              No completion time estimate available.{" "}
              <a
                href={`https://howlongtobeat.com/?q=${encodeURIComponent(game.name)}`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-steam-blue hover:underline"
              >
                Search HLTB
              </a>
            </div>
          )}

          {/* Reason */}
          <div className="mb-4 p-3 rounded-lg bg-steam-bg text-sm text-steam-text-dim">
            {game.reason}
          </div>

          {/* AI suggestion */}
          {game.confidence === "MEDIUM" && !aiSuggestion && (
            aiReady === false ? (
              <div className="mb-4 p-3 rounded-lg bg-yellow-900/20 border border-yellow-700/30 text-xs text-yellow-200/80">
                AI can provide a second opinion on this classification. Download the AI model in <strong className="text-yellow-200">Settings</strong> to enable this feature.
              </div>
            ) : (
              <button
                onClick={handleAskAI}
                disabled={aiLoading}
                className="mb-4 w-full py-2 px-3 rounded-lg text-sm border border-steam-blue/30 text-steam-blue hover:bg-steam-blue/10 transition-colors disabled:opacity-50"
              >
                {aiLoading ? "Asking AI..." : "Ask AI for a second opinion"}
              </button>
            )
          )}

          {aiSuggestion && (
            <div className="mb-4 p-3 rounded-lg bg-steam-blue/10 border border-steam-blue/20 text-sm">
              <div className="flex items-center gap-2 mb-1">
                <span className="text-steam-blue font-medium">
                  AI suggests: {CATEGORY_LABELS[aiSuggestion.suggested_category as CategoryKey] || aiSuggestion.suggested_category}
                </span>
                {!aiSuggestion.used_llm && (
                  <span className="text-xs text-steam-text-dim">(rules-based)</span>
                )}
              </div>
              <div className="text-xs text-steam-text-dim">
                {aiSuggestion.rationale}
              </div>
              {aiSuggestion.suggested_category !== game.category && (
                <button
                  onClick={() =>
                    onOverride(
                      game.appid,
                      aiSuggestion.suggested_category as CategoryKey
                    )
                  }
                  className="mt-2 py-1 px-3 rounded text-xs bg-steam-blue text-white hover:bg-steam-blue-hover transition-colors"
                >
                  Apply suggestion
                </button>
              )}
            </div>
          )}

          {/* Override buttons */}
          <div className="mb-4">
            <div className="text-xs text-steam-text-dim mb-2 uppercase tracking-wide font-medium">
              Change category
            </div>
            <div className="grid grid-cols-2 gap-2">
              {CATEGORIES.map((cat) => (
                <button
                  key={cat}
                  onClick={() => onOverride(game.appid, cat)}
                  disabled={cat === game.category}
                  className="py-2 px-3 rounded-lg text-sm font-medium transition-colors border disabled:opacity-30"
                  style={{
                    borderColor: CATEGORY_COLORS[cat],
                    color:
                      cat === game.category
                        ? CATEGORY_COLORS[cat]
                        : "#c7d5e0",
                    backgroundColor:
                      cat === game.category
                        ? `${CATEGORY_COLORS[cat]}20`
                        : "transparent",
                  }}
                >
                  {CATEGORY_LABELS[cat]}
                </button>
              ))}
            </div>
          </div>

          <button
            onClick={onClose}
            className="w-full py-2 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
