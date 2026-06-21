import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { DownloadProgress, LauncherStatus } from './api'

/** Subscribe to download progress events emitted by the Rust backend. */
export function listenDownloadProgress(
  callback: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>('download://progress', (event) => {
    callback(event.payload)
  })
}

/** Subscribe to backend operation status events. */
export function listenLauncherStatus(
  callback: (status: LauncherStatus) => void,
): Promise<UnlistenFn> {
  return listen<LauncherStatus>('launcher://status', (event) => {
    callback(event.payload)
  })
}
