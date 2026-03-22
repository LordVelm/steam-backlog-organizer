import { useState, useEffect, useRef } from "react";
import {
  getRecommendations,
  checkAiSetup,
  GameRecommendation,
  ChatMessage,
  STEAM_HEADER_URL,
  CATEGORY_COLORS,
} from "../lib/commands";

interface Props {
  onClose: () => void;
}

interface ChatEntry {
  role: "user" | "assistant";
  content: string;
  picks?: GameRecommendation[];
  usedLlm?: boolean;
}

export default function ChatPanel({ onClose }: Props) {
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<ChatEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [aiReady, setAiReady] = useState<boolean | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    checkAiSetup().then((s) => setAiReady(s.modelReady && s.serverReady)).catch(() => setAiReady(false));
  }, []);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [history, loading]);

  async function handleSend() {
    const message = input.trim();
    if (!message || loading) return;

    setInput("");
    const newHistory = [...history, { role: "user" as const, content: message }];
    setHistory(newHistory);
    setLoading(true);

    // Build clean chat history for the backend
    // Use pick data (not raw content) to avoid sending leaked JSON back to the model
    const chatHistory: ChatMessage[] = newHistory.slice(0, -1).map((entry) => {
      if (entry.role === "assistant") {
        // Reconstruct a clean assistant message from structured data
        const parts: string[] = [];
        if (entry.content) parts.push(entry.content);
        if (entry.picks && entry.picks.length > 0) {
          const picksText = entry.picks
            .map((p) => `Recommended: ${p.title} — ${p.reason}`)
            .join(". ");
          parts.push(picksText);
        }
        return { role: "assistant", content: parts.join("\n") };
      }
      return { role: entry.role, content: entry.content };
    });

    try {
      const response = await getRecommendations(message, chatHistory);
      const assistantContent = response.picks.length > 0
        ? (response.used_llm ? response.message || "Here are my picks:" : "Here are some suggestions from your backlog:")
        : (response.message || "I couldn't find a great match — try asking differently!");
      setHistory((h) => [
        ...h,
        {
          role: "assistant",
          content: assistantContent,
          picks: response.picks,
          usedLlm: response.used_llm,
        },
      ]);
    } catch (e) {
      setHistory((h) => [
        ...h,
        {
          role: "assistant",
          content: `Something went wrong: ${e}`,
        },
      ]);
    }

    setLoading(false);
  }

  const pendingChipRef = useRef<string | null>(null);

  function sendChip(text: string) {
    pendingChipRef.current = text;
    setInput(text);
  }

  // When input changes from a chip click, auto-send
  useEffect(() => {
    if (pendingChipRef.current && input === pendingChipRef.current) {
      pendingChipRef.current = null;
      handleSend();
    }
  }, [input]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  return (
    <div className="fixed inset-0 top-9 bg-black/60 flex items-end justify-center z-50 sm:items-center animate-fadeIn">
      <div className="bg-steam-surface rounded-t-xl sm:rounded-xl w-full max-w-lg mx-0 sm:mx-4 h-[80vh] sm:h-[600px] flex flex-col border border-steam-border animate-slideUp">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-steam-border">
          <h2 className="text-lg font-bold text-white">
            What should I play next?
          </h2>
          <button
            onClick={onClose}
            className="text-steam-text-dim hover:text-white transition-colors text-xl leading-none"
          >
            ×
          </button>
        </div>

        {/* Messages */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {aiReady === false && (
            <div className="p-3 rounded-lg bg-yellow-900/20 border border-yellow-700/30 text-xs text-yellow-200/80 mb-2">
              AI model not downloaded — recommendations will use basic rules. Download the AI model in <strong className="text-yellow-200">Settings</strong> for smarter, personalized picks.
            </div>
          )}

          {history.length === 0 && (
            <div className="text-center text-steam-text-dim text-sm py-8">
              <p className="mb-3">Ask me what to play!</p>
              <div className="space-y-2 text-xs">
                <SuggestionChip text="Something short I can finish tonight" onClick={sendChip} />
                <SuggestionChip text="Pick from my backlog" onClick={sendChip} />
                <SuggestionChip text="Something cozy I haven't started" onClick={sendChip} />
              </div>
            </div>
          )}

          {history.map((entry, i) => (
            <div key={i}>
              {entry.role === "user" ? (
                <div className="flex justify-end">
                  <div className="bg-steam-blue/20 text-white rounded-lg px-3 py-2 max-w-[80%] text-sm">
                    {entry.content}
                  </div>
                </div>
              ) : (
                <div className="space-y-2">
                  <div className="text-sm text-steam-text-dim">
                    {entry.content}
                    {entry.usedLlm === false && (
                      <span className="ml-2 text-xs opacity-60">
                        (rules-based)
                      </span>
                    )}
                  </div>
                  {entry.picks && entry.picks.length > 0 && (
                    <div className="space-y-2">
                      {entry.picks.map((pick) => (
                        <PickCard key={pick.appid} pick={pick} />
                      ))}
                    </div>
                  )}
                  {entry.picks && entry.picks.length === 0 && !entry.usedLlm && (
                    <div className="text-sm text-steam-text-dim">
                      No matching games found in your library.
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}

          {loading && (
            <div className="flex items-center gap-2 text-steam-text-dim text-sm">
              <div className="w-4 h-4 border-2 border-steam-blue border-t-transparent rounded-full animate-spin" />
              Thinking...
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* Input */}
        <div className="p-3 border-t border-steam-border">
          <div className="flex gap-2">
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="What are you in the mood for?"
              className="flex-1 px-3 py-2 rounded-lg bg-steam-bg border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
              disabled={loading}
            />
            <button
              onClick={handleSend}
              disabled={loading || !input.trim()}
              className="px-4 py-2 rounded-lg bg-steam-blue text-white font-medium hover:bg-steam-blue-hover transition-colors disabled:opacity-50 text-sm"
            >
              Send
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function PickCard({ pick }: { pick: GameRecommendation }) {
  const [imgError, setImgError] = useState(false);

  return (
    <div className="flex gap-3 p-2 rounded-lg bg-steam-surface-light/50 border border-steam-border">
      <div className="w-28 h-[52px] rounded overflow-hidden shrink-0 bg-steam-bg">
        {!imgError ? (
          <img
            src={STEAM_HEADER_URL(pick.appid)}
            alt={pick.title}
            className="w-full h-full object-cover"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-[10px] text-steam-text-dim p-1 text-center">
            {pick.title}
          </div>
        )}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-white truncate">
          {pick.title}
        </div>
        <div className="text-xs text-steam-text-dim mt-0.5">{pick.reason}</div>
      </div>
    </div>
  );
}

function SuggestionChip({
  text,
  onClick,
}: {
  text: string;
  onClick: (text: string) => void;
}) {
  return (
    <button
      onClick={() => onClick(text)}
      className="block mx-auto px-3 py-1.5 rounded-full border border-steam-border text-steam-text-dim hover:text-white hover:border-steam-blue transition-colors"
    >
      {text}
    </button>
  );
}
