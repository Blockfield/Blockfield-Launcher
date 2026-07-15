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
  forgeOk: boolean
}

// ── Download progress ────────────────────────────────────────────

/** Progress payload emitted by the backend during download. */
export interface DownloadProgress {
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

export interface ServerStatus {
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

export async function fetchServerStatus(): Promise<ServerStatus> {
  const api = (
    import.meta.env.VITE_BLOCKFIELD_API_URL ?? 'http://localhost:3000/api/launcher/v1'
  ).replace(/\/$/, '')
  const started = performance.now()
  const response = await fetch(`${api}/server-status`)
  if (!response.ok) throw new Error(`Server status failed: ${response.status}`)
  return { ...(await response.json()), apiLatencyMs: Math.round(performance.now() - started) }
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
  updateDescription?: string
  update_description?: string
  settingsPreferences?: string
  settings_preferences?: string
  translations?: Record<string, Record<string, string>>
  features?: LauncherContentFeature[]
  feed?: LauncherContentFeedEntry[]
}

// ── Launcher config ──────────────────────────────────────────────

/** Launcher configuration persisted to disk. */
export interface LauncherConfig {
  gameDir: string
  javaPath: string
  ramMb: number
  autoUpdate: boolean
  lang: string
}
