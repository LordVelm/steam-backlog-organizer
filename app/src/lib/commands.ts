import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// -- Types --

export type CategoryKey =
  | "COMPLETED"
  | "IN_PROGRESS"
  | "ENDLESS"
  | "NOT_A_GAME";

export interface OwnedGame {
  appid: number;
  name: string;
  playtime_hours: number;
  achievements?: {
    total: number;
    achieved: number;
    percentage: number;
    names_achieved: string[];
  };
}

export interface Classification {
  appid: number;
  name: string;
  category: CategoryKey;
  confidence: string;
  reason: string;
}

export interface ConfigStatus {
  configured: boolean;
  steam_id: string | null;
}

export interface CategorySummary {
  completed: number;
  in_progress: number;
  endless: number;
  not_a_game: number;
  total: number;
}

// -- Commands --

export function checkConfig(): Promise<ConfigStatus> {
  return invoke("check_config");
}

export function saveConfig(
  apiKey: string,
  steamId: string
): Promise<void> {
  return invoke("save_config", { apiKey, steamId });
}

export function fetchLibrary(): Promise<OwnedGame[]> {
  return invoke("fetch_library");
}

export function fetchStoreDetails(): Promise<void> {
  return invoke("fetch_store_details");
}

export function classifyGames(): Promise<Classification[]> {
  return invoke("classify_games");
}

export function getClassifications(): Promise<Classification[]> {
  return invoke("get_classifications");
}

export function setOverride(
  appid: string,
  category: CategoryKey
): Promise<void> {
  return invoke("set_override", { appid, category });
}

export function removeOverride(appid: string): Promise<void> {
  return invoke("remove_override", { appid });
}

export function getOverrides(): Promise<Record<string, string>> {
  return invoke("get_overrides");
}

export function getSummary(): Promise<CategorySummary> {
  return invoke("get_summary");
}

// -- Steam Collections --

export interface SteamAccount {
  id: string;
  path: string;
}

export function checkSteamRunning(): Promise<boolean> {
  return invoke("check_steam_running");
}

export function getSteamAccounts(): Promise<SteamAccount[]> {
  return invoke("get_steam_accounts");
}

export function writeToSteam(accountPath: string): Promise<void> {
  return invoke("write_to_steam", { accountPath });
}

// -- AI (bundled llama-server) --

export interface SetupStatus {
  modelReady: boolean;
  serverReady: boolean;
}

export interface DownloadProgress {
  stage: string;
  downloaded: number;
  total: number;
  percent: number;
}

export interface GpuStatus {
  gpuDetected: boolean;
  cudaBuild: boolean;
  usingGpu: boolean;
}

export interface GameRecommendation {
  appid: number;
  title: string;
  reason: string;
}

export interface RecommendResponse {
  picks: GameRecommendation[];
  used_llm: boolean;
  message: string;
}

export interface AmbiguityResponse {
  suggested_category: string;
  rationale: string;
  used_llm: boolean;
}

export function checkAiSetup(): Promise<SetupStatus> {
  return invoke("check_ai_setup");
}

export function setupAi(): Promise<void> {
  return invoke("setup_ai");
}

export function getGpuStatus(): Promise<GpuStatus> {
  return invoke("get_gpu_status");
}

export function setGpuEnabled(enabled: boolean): Promise<void> {
  return invoke("set_gpu_enabled", { enabled });
}

export function onDownloadProgress(
  callback: (progress: DownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("download-progress", (event) => {
    callback(event.payload);
  });
}

export interface SyncProgress {
  step: string;
  current: number;
  total: number;
}

export function onSyncProgress(
  callback: (progress: SyncProgress) => void
): Promise<UnlistenFn> {
  return listen<SyncProgress>("sync-progress", (event) => {
    callback(event.payload);
  });
}

export function cancelSync(): Promise<void> {
  return invoke("cancel_sync");
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export function getRecommendations(
  message: string,
  history: ChatMessage[]
): Promise<RecommendResponse> {
  return invoke("get_recommendations", { request: { message, history } });
}

export function getAmbiguitySuggestion(
  appid: number
): Promise<AmbiguityResponse> {
  return invoke("get_ambiguity_suggestion", { request: { appid } });
}

export function exportJson(): Promise<string> {
  return invoke("export_json");
}

// -- HowLongToBeat --

export interface HltbData {
  appid: number;
  hltb_name: string;
  main_story: number | null;
  main_extra: number | null;
  completionist: number | null;
}

export type LengthBucket =
  | "Quick"
  | "Short"
  | "Medium"
  | "Long"
  | "VeryLong"
  | "Unknown";

export const LENGTH_BUCKET_LABELS: Record<LengthBucket, string> = {
  Quick: "Quick  < 2h",
  Short: "Short  2–8h",
  Medium: "Medium  8–20h",
  Long: "Long  20–50h",
  VeryLong: "Epic  50h+",
  Unknown: "Unknown",
};

export const LENGTH_BUCKET_COLORS: Record<LengthBucket, string> = {
  Quick: "#4ade80",
  Short: "#22d3ee",
  Medium: "#fbbf24",
  Long: "#f97316",
  VeryLong: "#ef4444",
  Unknown: "#6b7280",
};

export function getLengthBucket(mainStory: number | null): LengthBucket {
  if (mainStory === null) return "Unknown";
  if (mainStory < 2) return "Quick";
  if (mainStory < 8) return "Short";
  if (mainStory < 20) return "Medium";
  if (mainStory < 50) return "Long";
  return "VeryLong";
}

/** Format hours for display: "45m", "4.5h", "48h". */
export function formatHours(hours: number): string {
  if (hours < 1) return `${Math.round(hours * 60)}m`;
  if (hours < 10) return `${hours.toFixed(1)}h`;
  return `${Math.round(hours)}h`;
}

export function fetchHltbData(): Promise<void> {
  return invoke("fetch_hltb_data");
}

export function getHltbData(): Promise<Record<string, HltbData>> {
  return invoke("get_hltb_data");
}

export function onHltbProgress(
  callback: (progress: SyncProgress) => void
): Promise<UnlistenFn> {
  return listen<SyncProgress>("hltb-progress", (event) => {
    callback(event.payload);
  });
}

// -- Helpers --

export const CATEGORY_LABELS: Record<CategoryKey, string> = {
  COMPLETED: "Completed",
  IN_PROGRESS: "In Progress / Backlog",
  ENDLESS: "Endless",
  NOT_A_GAME: "Not a Game",
};

export const CATEGORY_COLORS: Record<CategoryKey, string> = {
  COMPLETED: "#4CAF50",
  IN_PROGRESS: "#FFC107",
  ENDLESS: "#2196F3",
  NOT_A_GAME: "#9E9E9E",
};

export const STEAM_HEADER_URL = (appid: number) =>
  `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/header.jpg`;

export const STEAM_CAPSULE_URL = (appid: number) =>
  `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/capsule_231x87.jpg`;
