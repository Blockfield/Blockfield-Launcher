import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { DownloadProgress, LauncherStatus } from './api'

async function listenUntilUnsubscribed<T>(
  event: string,
  callback: (payload: T) => void,
): Promise<UnlistenFn> {
  let active = true
  const unlisten = await listen<T>(event, ({ payload }) => {
    if (active) callback(payload)
  })
  return () => {
    // Native unsubscription is asynchronous; queued callbacks must stop immediately.
    active = false
    unlisten()
  }
}

/** Subscribe to download progress events emitted by the Rust backend. */
export function listenDownloadProgress(
  callback: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listenUntilUnsubscribed('download://progress', callback)
}

/** Subscribe to backend operation status events. */
export function listenLauncherStatus(
  callback: (status: LauncherStatus) => void,
): Promise<UnlistenFn> {
  return listenUntilUnsubscribed('launcher://status', callback)
}
