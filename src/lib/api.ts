// ── Manifest types (mirrors server manifest + shared crate) ─────────

/** A single file entry in the modpack manifest. */
export interface ManifestFileEntry {
  /** Relative path within the game directory, e.g. "mods/battlefield-core.jar" */
  path: string
  /** File size in bytes */
  size: number
  /** Hex-encoded SHA-256 hash */
  sha256: string
  /** Download URL */
  url: string
}

/** Required Java runtime info. */
export interface JavaInfo {
  version: string
  platform: string
  url: string
  sha256: string
  size: number
}

/** The remote modpack manifest. */
export interface ModpackManifest {
  version: string
  minecraftVersion: string
  files: ManifestFileEntry[]
  totalSize: number
  releaseDate: string
  prune?: string[]
  java?: JavaInfo
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

// ── Default constants ────────────────────────────────────────────

export const API_BASE_URL = 'https://play.blockfield.gg/api/launcher/v1'
export const MANIFEST_URL = `${API_BASE_URL}/manifest.json`
