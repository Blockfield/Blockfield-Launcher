import type { GameStatus } from './game-state'
import { invoke } from '@tauri-apps/api/core'
import { useSyncExternalStore } from 'react'
import type { LauncherConfig, VersionCheckResult } from './api'

// Single source of truth for the installed/remote pack version: every screen reads this
// store instead of keeping its own copy, so "Играть" and "Обновления" can't disagree.
let cached: VersionCheckResult | null = null
let pending: Promise<VersionCheckResult> | null = null
let generation = 0
const listeners = new Set<() => void>()

function notify() {
  for (const listener of listeners) listener()
}

function setCached(result: VersionCheckResult) {
  cached = result
  notify()
}

export function invalidateModpackCheck() {
  generation++
  pending = null
  cached = null
  notify()
}

export function subscribeModpackVersion(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function getModpackVersionSnapshot(): VersionCheckResult | null {
  return cached
}

/** Live view of the shared version-check result; updates whenever any screen re-syncs. */
export function useModpackVersion(): VersionCheckResult | null {
  return useSyncExternalStore(subscribeModpackVersion, getModpackVersionSnapshot)
}

export function checkModpack(verify = false): Promise<VersionCheckResult> {
  if (pending) return pending
  if (cached && !verify) return Promise.resolve(cached)
  const requestedGeneration = generation
  const request: Promise<VersionCheckResult> = (async () => {
    const config = await invoke<LauncherConfig>('load_settings')
    const result = await invoke<VersionCheckResult>('check_modpack_version')
    if (requestedGeneration !== generation) throw new Error('Профиль изменён. Повторите проверку.')
    if ((verify || config.autoUpdate) && !result.needsUpdate && result.javaOk && result.loaderOk) {
      const game = await invoke<GameStatus>('game_status')
      if (requestedGeneration !== generation)
        throw new Error('Профиль изменён. Повторите проверку.')
      if (game.phase !== 'idle') {
        if (verify) throw new Error('Закройте игру перед проверкой файлов')
        return result
      }
      const failed = await invoke<string[]>('verify_files', {
        profile: config.activeProfile ?? 'game',
      })
      if (failed.length) throw new Error(`Не удалось проверить файлы: ${failed.join(', ')}`)
    }
    if (requestedGeneration !== generation) throw new Error('Профиль изменён. Повторите проверку.')
    setCached(result)
    return result
  })().finally(() => {
    if (pending === request) pending = null
  })
  pending = request
  return request
}
