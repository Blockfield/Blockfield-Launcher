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

// ── Launcher config ──────────────────────────────────────────────

/** Launcher configuration persisted to disk. */
export interface LauncherConfig {
  gameDir: string
  javaPath: string
  ramMb: number
  autoUpdate: boolean
  lang: string
}
