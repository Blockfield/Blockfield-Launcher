import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { DownloadProgress } from './api'

/** Subscribe to download progress events emitted by the Rust backend. */
export function listenDownloadProgress(
  callback: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>('download://progress', (event) => {
    callback(event.payload)
  })
}

/** Subscribe to launch status events. */
export function listenLaunchStatus(
  callback: (status: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('launch://status', (event) => {
    callback(event.payload)
  })
}
