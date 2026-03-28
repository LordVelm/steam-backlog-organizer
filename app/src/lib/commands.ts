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

// -- HLTB --

export interface HltbEntry {
  main_story_hours: number | null;
  main_extra_hours: number | null;
  completionist_hours: number | null;
  hltb_game_id: string | null;
  match_status: string;
  fetched_at: number;
}

export interface HltbComplete {
  matched: number;
  total: number;
  new_matches: number;
}

export function getHltbCache(): Promise<Record<string, HltbEntry>> {
  return invoke("get_hltb_cache");
}

export function fetchHltbData(): Promise<void> {
  return invoke("fetch_hltb_data");
}

export function onHltbComplete(
  callback: (data: HltbComplete) => void
): Promise<UnlistenFn> {
  return listen<HltbComplete>("hltb-complete", (event) => {
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
