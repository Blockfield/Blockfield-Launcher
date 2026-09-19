/** Required Java runtime info. */
export interface JavaInfo {
  version: string
  platform: string
  url: string
  sha256: string
  size: number
}

/** Result of checking for modpack updates. */
export interface VersionCheckResult {
  needsUpdate: boolean
  remoteVersion: string
  installedVersion: string
  mirror: string
  fileCount: number
  totalSize: number
  java?: JavaInfo
  javaOk: boolean
  loaderOk: boolean
}

// ── Download progress ────────────────────────────────────────────────────────

/** Progress payload emitted by the backend during download. */
export interface DownloadProgress {
  unit?: 'bytes' | 'files'
  filePath: string
  fileIndex: number
  fileCount: number
  bytesDownloaded: number
  fileBytesTotal: number
  totalBytesDownloaded: number
  totalBytesAll: number
  speedBytesPerSec: number
}

/** Current backend operation. */
export interface LauncherStatus {
  phase: string
  message: string
  cancelable: boolean
}

export interface RoomBattle {
  red: number
  blue: number
  points: {
    id: string
    name: string
    owner: string
    claiming: string
    capturing: string
    progress: number
  }[]
}

export interface GameRoom {
  battle?: RoomBattle | null
  id: string
  name: string
  mode: string
  map: string
  phase: string
  players: number
  joinable: boolean
  /** `casual` or `ranked`; absent on older servers, which only ever ran Casual rooms. */
  format?: 'casual' | 'ranked' | null
  /** `open` (join now), `waiting` (queue only) or `closed` (full or campaign running). */
  admission?: 'open' | 'waiting' | 'closed' | null
  ready?: { ready: number; required: number } | null
}

export interface ServerStatus {
  rooms?: GameRoom[] | null
  /** `false` while the game server behind the proxy restarts; `null` when the site API is unreachable. */
  gameAvailable?: boolean | null
  /** Host controller maintenance/workshop window; implies `gameAvailable === false` and no rooms. */
  maintenance?: { message: string; since?: number | null } | null
  online: boolean
  playersOnline: number | null
  playersMax: number | null
  minecraftVersion: string | null
  motd: unknown
  host: string
  port: number
  regionCode: string
  locationName: string
  serverLatencyMs: number | null
  checkedAt: number
  apiLatencyMs: number
}

/** The Rust side pings the game server directly (Server List Ping); no web API involved. */
export async function fetchServerStatus(): Promise<ServerStatus> {
  const { invoke } = await import('@tauri-apps/api/core')
  const started = performance.now()
  const status = await invoke<Omit<ServerStatus, 'apiLatencyMs'>>('server_status')
  return { ...status, apiLatencyMs: Math.round(performance.now() - started) }
}

/** Open an external URL in the system's default browser. */
export async function openExternalUrl(url: string): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_url', { url })
  } catch {
    if (typeof window !== 'undefined' && typeof window.open === 'function') {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }
}

export interface LauncherContentFeature {
  icon?: string
  title?: string
  desc?: string
}

export interface LauncherContentFeedEntry {
  tag?: string
  tone?: string
  date?: string
  title?: string
  body?: string
}

export interface LauncherContent {
  [key: string]: unknown
  brand?: string
  brandSubtitle?: string
  brand_subtitle?: string
  chromeTitle?: string
  chrome_title?: string
  operationName?: string
  operation_name?: string
  season?: string
  description?: string
  serverName?: string
  server_name?: string
  serverIp?: string
  server_ip?: string
  copyright?: string
  loginSector?: string
  login_sector?: string
  loginSlogan?: string
  login_slogan?: string
  supportLabel?: string
  support_label?: string
  supportUrl?: string
  support_url?: string
  updateDescription?: string
  update_description?: string
  settingsPreferences?: string
  settings_preferences?: string
  translations?: Record<string, Record<string, string>>
  features?: LauncherContentFeature[]
  feed?: LauncherContentFeedEntry[]
}

// ── Launcher config ──────────────────────────────────────────────────────────

/** Launcher configuration persisted to disk. */
export interface LauncherConfig {
  gameDir: string
  javaPath: string
  maxRamMb?: number
  ramMb: number
  autoUpdate: boolean
  lang: string
  username?: string
  preLaunchCommand: string
  postExitCommand: string
  hideWhilePlaying: boolean
  discordPresence?: boolean
}

/** Drasl account from `account_status`, `login`, `register`, `logout` and `change_password`. */
export interface AccountStatus {
  loggedIn: boolean
  username: string
  uuid: string
  needsPassword: boolean
}

/** Result of `upload_skin`: texture URLs on the skin server. */
export interface SkinUploadResult {
  skinUrl: string | null
  capeUrl: string | null
}

/** Textures shown in the 3D skin preview, as `data:` URLs. */
export interface SkinPreview {
  skin: string | null
  cape: string | null
  slim: boolean
  source: 'custom' | 'mojang' | 'none'
}
