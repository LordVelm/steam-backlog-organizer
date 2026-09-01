import { useCallback, useEffect, useRef, useState } from "react";
import {
  DiscoverFilters,
  DiscoverItem,
  TasteSetupStatus,
  WishlistScoredItem,
  STEAM_CAPSULE_URL,
  getDiscoverFeed,
  getWishlistScored,
} from "../lib/commands";
import { open } from "@tauri-apps/plugin-shell";

interface Props {
  tasteSetup: TasteSetupStatus | null;
  profileReady: boolean;
  lowConfidence: boolean;
  signalCount: number;
}

const DEFAULT_FILTERS: DiscoverFilters = {
  minReviewPct: 70,
  minReviews: 200,
  excludeOwned: true,
  includeAdult: false,
};

const RELEASE_WINDOWS: { label: string; after?: number; before?: number }[] = [
  { label: "Any year" },
  { label: "Last 2 years", after: new Date().getFullYear() - 2 },
  { label: "Last 5 years", after: new Date().getFullYear() - 5 },
  { label: "Classics (pre-2015)", before: 2014 },
];

export default function DiscoverView({ tasteSetup, profileReady, lowConfidence, signalCount }: Props) {
  const [tab, setTab] = useState<"foryou" | "wishlist">("foryou");
  const [filters, setFilters] = useState<DiscoverFilters>(DEFAULT_FILTERS);
  const [releaseWindow, setReleaseWindow] = useState(0);
  const [items, setItems] = useState<DiscoverItem[]>([]);
  const [wishlist, setWishlist] = useState<WishlistScoredItem[] | null>(null);
  const [wishlistError, setWishlistError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (tab !== "wishlist" || wishlist !== null || !profileReady) return;
    getWishlistScored()
      .then(setWishlist)
      .catch((e) => setWishlistError(String(e)));
  }, [tab, wishlist, profileReady]);

  const requestToken = useRef(0);

  const load = useCallback(async () => {
    if (!profileReady) return;
    const token = ++requestToken.current;
    setLoading(true);
    setError(null);
    try {
      const win = RELEASE_WINDOWS[releaseWindow];
      const feed = await getDiscoverFeed({
        ...filters,
        releasedAfterYear: win.after,
        releasedBeforeYear: win.before,
      });
      // A newer request superseded this one — drop the stale result
      if (token !== requestToken.current) return;
      setItems(feed);
    } catch (e) {
      if (token !== requestToken.current) return;
      setError(String(e));
    }
    if (token === requestToken.current) setLoading(false);
  }, [filters, releaseWindow, profileReady]);

  // Debounce: the rating slider fires per step — don't scan 60k games per tick
  useEffect(() => {
    const t = setTimeout(load, 150);
    return () => clearTimeout(t);
  }, [load]);

  if (!tasteSetup?.catalogInstalled) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="max-w-md text-center">
          <div className="text-4xl mb-4">◈</div>
          <h2 className="font-display text-xl text-white mb-2">
            {tasteSetup?.loading ? "Loading game catalog..." : "Catalog unavailable"}
          </h2>
          <p className="text-sm text-steam-text-dim">
            {tasteSetup?.loading
              ? "Discover works fully offline — one moment."
              : "The bundled game catalog failed to load. Try reinstalling Gamekeeper."}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Tabs */}
      <div className="px-6 pt-3 flex items-center gap-1">
        {(
          [
            { key: "foryou", label: "For You" },
            { key: "wishlist", label: "Wishlist" },
          ] as const
        ).map((t) => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`px-3 py-1.5 rounded-lg text-sm transition-colors ${
              tab === t.key
                ? "bg-steam-surface-light text-white"
                : "text-steam-text-dim hover:text-white"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Filter bar */}
      {tab === "foryou" && (
      <div className="px-6 py-3 border-b border-steam-border flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
        <label className="flex items-center gap-2 text-steam-text-dim">
          Rating ≥
          <input
            type="range"
            min={50}
            max={95}
            step={5}
            value={filters.minReviewPct}
            onChange={(e) =>
              setFilters((f) => ({ ...f, minReviewPct: Number(e.target.value) }))
            }
            className="w-24 accent-steam-blue"
          />
          <span className="font-mono text-steam-text w-9">{filters.minReviewPct}%</span>
        </label>

        <label className="flex items-center gap-2 text-steam-text-dim">
          Min reviews
          <select
            value={filters.minReviews}
            onChange={(e) =>
              setFilters((f) => ({ ...f, minReviews: Number(e.target.value) }))
            }
            className="bg-steam-surface-light border border-steam-border rounded-md px-2 py-1 text-steam-text"
          >
            {[50, 200, 1000, 10000].map((n) => (
              <option key={n} value={n}>
                {n.toLocaleString()}+
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-steam-text-dim">
          Released
          <select
            value={releaseWindow}
            onChange={(e) => setReleaseWindow(Number(e.target.value))}
            className="bg-steam-surface-light border border-steam-border rounded-md px-2 py-1 text-steam-text"
          >
            {RELEASE_WINDOWS.map((w, i) => (
              <option key={w.label} value={i}>
                {w.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-1.5 text-steam-text-dim cursor-pointer">
          <input
            type="checkbox"
            checked={filters.excludeOwned}
            onChange={(e) =>
              setFilters((f) => ({ ...f, excludeOwned: e.target.checked }))
            }
            className="accent-steam-blue"
          />
          Hide owned
        </label>
      </div>
      )}

      {lowConfidence && (
        <div className="mx-6 mt-3 px-4 py-2 rounded-lg bg-steam-surface-light border border-steam-border text-sm text-steam-text-dim">
          Your taste profile is based on only {signalCount} played games — recommendations
          sharpen as you play more.
        </div>
      )}

      {/* Feed */}
      <div className="flex-1 overflow-y-auto p-6 pt-4">
        {tab === "wishlist" && (
          <div className="space-y-3 max-w-3xl">
            {wishlistError && (
              <div className="p-4 rounded-xl bg-steam-surface border border-steam-border text-sm text-steam-text-dim">
                {wishlistError.includes("EMPTY_OR_PRIVATE") ? (
                  <>
                    Your wishlist looks empty — or your Steam profile's game details
                    are private.{" "}
                    <button
                      className="text-steam-blue hover:underline"
                      onClick={() =>
                        open("https://steamcommunity.com/my/edit/settings")
                      }
                    >
                      Check privacy settings
                    </button>
                  </>
                ) : (
                  <>Couldn't load your wishlist: {wishlistError}</>
                )}
              </div>
            )}
            {!wishlistError && wishlist === null && (
              <p className="text-sm text-steam-text-dim animate-pulse">
                Scoring your wishlist against your taste...
              </p>
            )}
            {wishlist?.map((item, i) => (
              <button
                key={item.appid}
                onClick={() => open(`https://store.steampowered.com/app/${item.appid}`)}
                className="w-full flex gap-4 p-3 rounded-xl bg-steam-surface border border-steam-border hover:border-steam-blue/50 hover:bg-steam-surface-light transition-colors text-left group"
              >
                <span className="font-mono text-xs text-steam-text-dim w-6 pt-1 shrink-0">
                  {item.unscored ? "—" : i + 1}
                </span>
                <img
                  src={STEAM_CAPSULE_URL(item.appid)}
                  alt=""
                  loading="lazy"
                  className="w-[184px] aspect-[184/69] object-cover rounded-md shrink-0 bg-steam-surface-light"
                  onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-2">
                    <h3 className="font-medium text-white truncate group-hover:text-steam-blue transition-colors">
                      {item.name}
                    </h3>
                    {!item.unscored && (
                      <span className="font-mono text-xs text-steam-text-dim shrink-0">
                        {Math.round(item.simScore * 100)}% match · {item.reviewPct}%
                        positive
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-steam-text-dim mt-0.5 truncate">{item.reason}</p>
                  {item.warning && (
                    <p className="text-xs text-amber-400 mt-1.5">⚠ {item.warning}</p>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}
        {tab === "foryou" && (
          <>
        {error && (
          <p className="text-sm text-red-400 mb-4">Couldn't load recommendations: {error}</p>
        )}
        {loading && items.length === 0 && (
          <p className="text-sm text-steam-text-dim animate-pulse">Scanning the catalog against your taste...</p>
        )}
        {!loading && !error && items.length === 0 && (
          <p className="text-sm text-steam-text-dim">
            Nothing matches these filters — try loosening them.
          </p>
        )}
        <div className="space-y-3 max-w-3xl">
          {items.map((item, i) => (
            <button
              key={item.appid}
              onClick={() => open(`https://store.steampowered.com/app/${item.appid}`)}
              className="w-full flex gap-4 p-3 rounded-xl bg-steam-surface border border-steam-border hover:border-steam-blue/50 hover:bg-steam-surface-light transition-colors text-left group"
            >
              <span className="font-mono text-xs text-steam-text-dim w-6 pt-1 shrink-0">
                {i + 1}
              </span>
              <img
                src={STEAM_CAPSULE_URL(item.appid)}
                alt=""
                loading="lazy"
                className="w-[184px] aspect-[184/69] object-cover rounded-md shrink-0 bg-steam-surface-light"
                onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <h3 className="font-medium text-white truncate group-hover:text-steam-blue transition-colors">
                    {item.name}
                  </h3>
                  <span className="font-mono text-xs text-steam-text-dim shrink-0">
                    {item.reviewPct}% · {item.reviewTotal.toLocaleString()} reviews
                    {item.releaseYear > 0 && ` · ${item.releaseYear}`}
                  </span>
                </div>
                <p className="text-sm text-steam-text-dim mt-0.5 truncate">{item.reason}</p>
                <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
                  {item.tags.slice(0, 4).map((t) => (
                    <span
                      key={t}
                      className="px-1.5 py-0.5 rounded text-[11px] bg-steam-surface-light text-steam-text-dim"
                    >
                      {t}
                    </span>
                  ))}
                </div>
                {item.warning && (
                  <p className="text-xs text-amber-400 mt-1.5">⚠ {item.warning}</p>
                )}
              </div>
            </button>
          ))}
        </div>
          </>
        )}
      </div>
    </div>
  );
}
