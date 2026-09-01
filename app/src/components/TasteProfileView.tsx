import { useEffect, useState } from "react";
import {
  TasteProfile,
  TasteSetupStatus,
  STEAM_CAPSULE_URL,
  checkAiSetup,
  getTasteProse,
} from "../lib/commands";

interface Props {
  profile: TasteProfile | null;
  tasteSetup: TasteSetupStatus | null;
  error: string | null;
}

function confidenceLabel(c: TasteProfile["confidence"]): string {
  switch (c) {
    case "high":
      return "High confidence";
    case "medium":
      return "Medium confidence";
    default:
      return "Low confidence — play more games to sharpen it";
  }
}

function formatDatasetDate(d: number | null): string {
  if (!d) return "";
  const s = String(d);
  return `${s.slice(0, 4)}-${s.slice(4, 6)}-${s.slice(6, 8)}`;
}

export default function TasteProfileView({ profile, tasteSetup, error }: Props) {
  const [prose, setProse] = useState<string | null>(null);
  const [proseState, setProseState] = useState<"idle" | "loading" | "unavailable">("idle");

  useEffect(() => {
    if (!profile) return;
    let cancelled = false;
    (async () => {
      try {
        const setup = await checkAiSetup();
        if (!(setup.modelReady && setup.serverReady)) {
          if (!cancelled) setProseState("unavailable");
          return;
        }
        setProseState("loading");
        const text = await getTasteProse();
        if (!cancelled) {
          setProse(text);
          setProseState("idle");
        }
      } catch {
        if (!cancelled) setProseState("unavailable");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [profile?.computedAt]);

  if (!profile) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="max-w-md text-center">
          <div className="text-4xl mb-4">◉</div>
          <h2 className="font-display text-xl text-white mb-2">
            {error ? "Profile unavailable" : "Computing your taste profile..."}
          </h2>
          <p className="text-sm text-steam-text-dim">
            {error
              ? error.includes("LIBRARY_NOT_LOADED")
                ? "Sync your library first — the profile is built from what you actually play."
                : error.includes("CATALOG_NOT_READY")
                  ? "The game catalog is still loading."
                  : error
              : "Reading your playtime, completions, and habits."}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="max-w-3xl space-y-8">
        <header>
          <h1 className="font-display text-2xl text-white">What your library says about you</h1>
          <p className="text-sm text-steam-text-dim mt-1">
            Built from {profile.signalCount} played games · {confidenceLabel(profile.confidence)}
            {tasteSetup?.catalogDatasetDate
              ? ` · catalog ${formatDatasetDate(tasteSetup.catalogDatasetDate)}`
              : ""}
          </p>
        </header>

        {/* Tag affinities */}
        <section>
          <h2 className="text-sm font-semibold text-steam-text-dim tracking-wide uppercase mb-3">
            Taste signature
          </h2>
          <div className="space-y-2">
            {profile.topTags.map((t) => (
              <div key={t.tag} className="flex items-center gap-3">
                <span className="w-40 text-sm text-steam-text truncate shrink-0">{t.tag}</span>
                <div className="flex-1 h-2 rounded-full bg-steam-surface-light overflow-hidden">
                  <div
                    className="h-full rounded-full bg-steam-blue transition-all"
                    style={{ width: `${Math.max(t.weight * 100, 2)}%` }}
                  />
                </div>
                <span className="font-mono text-xs text-steam-text-dim w-8 text-right shrink-0">
                  {Math.round(t.weight * 100)}
                </span>
              </div>
            ))}
          </div>
        </section>

        {/* Anchor games */}
        <section>
          <h2 className="text-sm font-semibold text-steam-text-dim tracking-wide uppercase mb-3">
            The games that define it
          </h2>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
            {profile.anchorGames.map((a) => (
              <div key={a.appid} className="group">
                <img
                  src={STEAM_CAPSULE_URL(a.appid)}
                  alt={a.name}
                  loading="lazy"
                  className="w-full aspect-[184/69] object-cover rounded-md bg-steam-surface-light"
                  onError={(e) => ((e.target as HTMLImageElement).style.opacity = "0.2")}
                />
                <p className="text-xs text-steam-text-dim mt-1 truncate" title={a.name}>
                  {a.name}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* Anti-clusters */}
        <section>
          <h2 className="text-sm font-semibold text-steam-text-dim tracking-wide uppercase mb-3">
            You tend to bounce off
          </h2>
          {profile.antiClusters.length === 0 ? (
            <p className="text-sm text-steam-text-dim">
              No clear bounce patterns yet. When you repeatedly drop games of the same
              kind, Gamekeeper starts warning you before you buy another one.
            </p>
          ) : (
            <div className="space-y-3">
              {profile.antiClusters.map((c) => (
                <div
                  key={c.label}
                  className="p-4 rounded-xl bg-steam-surface border border-steam-border"
                >
                  <div className="flex items-center gap-2">
                    <span className="text-amber-400">⚠</span>
                    <h3 className="font-medium text-white">{c.label}</h3>
                    <span className="text-xs text-steam-text-dim">
                      {c.tags.slice(1).join(" · ")}
                    </span>
                  </div>
                  <p className="text-sm text-steam-text-dim mt-2">
                    {c.bounced
                      .map(
                        (b) =>
                          `${b.name} (${b.playtimeHours.toFixed(1)}h${b.kind === "abandoned" ? ", abandoned" : ""})`
                      )
                      .join(" · ")}
                  </p>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* AI-written summary (only when a model tier is installed) */}
        {(prose || proseState === "loading") && (
          <section>
            <h2 className="text-sm font-semibold text-steam-text-dim tracking-wide uppercase mb-3">
              In other words
            </h2>
            {proseState === "loading" ? (
              <p className="text-sm text-steam-text-dim animate-pulse">
                Your local AI is writing your profile...
              </p>
            ) : (
              <div className="p-4 rounded-xl bg-steam-surface border border-steam-border">
                {prose!.split(/\n\n+/).map((p, i) => (
                  <p
                    key={i}
                    className="text-sm text-steam-text leading-relaxed [&:not(:last-child)]:mb-3"
                  >
                    {p}
                  </p>
                ))}
              </div>
            )}
          </section>
        )}
      </div>
    </div>
  );
}
