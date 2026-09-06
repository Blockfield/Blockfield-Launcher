import type { GameStatus } from './game-state'
import { invoke } from '@tauri-apps/api/core'
import type { LauncherConfig, VersionCheckResult } from './api'

let cached: VersionCheckResult | null = null
let pending: Promise<VersionCheckResult> | null = null

export function invalidateModpackCheck() {
  cached = null
}

export function checkModpack(verify = false): Promise<VersionCheckResult> {
  if (pending) return pending
  if (cached && !verify) return Promise.resolve(cached)
  pending = (async () => {
    const config = await invoke<LauncherConfig>('load_settings')
    const result = await invoke<VersionCheckResult>('check_modpack_version')
    if ((verify || config.autoUpdate) && !result.needsUpdate && result.javaOk && result.loaderOk) {
      const game = await invoke<GameStatus>('game_status')
      if (game.phase !== 'idle') {
        if (verify) throw new Error('Закройте игру перед проверкой файлов')
        return result
      }
      const failed = await invoke<string[]>('verify_files')
      if (failed.length) throw new Error(`Не удалось проверить файлы: ${failed.join(', ')}`)
    }
    cached = result
    return result
  })().finally(() => {
    pending = null
  })
  return pending
}
