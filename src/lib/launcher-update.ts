import { Channel, invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { relaunch } from '@tauri-apps/plugin-process'
import { useSyncExternalStore } from 'react'

interface LauncherUpdate {
  currentVersion: string
  version: string
  body: string | null
}

type Phase =
  | 'idle'
  | 'checking'
  | 'current'
  | 'available'
  | 'downloading'
  | 'verifying'
  | 'installing'
  | 'installed'
  | 'restarting'
  | 'error'
interface Progress {
  phase: 'downloading' | 'verifying' | 'installing'
  downloaded: number
  total: number | null
}
export interface LauncherUpdateState {
  phase: Phase
  currentVersion: string
  update: LauncherUpdate | null
  downloaded: number
  total: number | null
  checkedAt: number | null
  error: string | null
  installed: boolean
}
let state: LauncherUpdateState = {
  phase: 'idle',
  currentVersion: '—',
  update: null,
  downloaded: 0,
  total: null,
  checkedAt: null,
  error: null,
  installed: false,
}
const listeners = new Set<() => void>()
let pending: Promise<void> | null = null
export const getLauncherUpdate = () => state
const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}
export const useLauncherUpdate = () => useSyncExternalStore(subscribe, getLauncherUpdate)
function patch(next: Partial<LauncherUpdateState>) {
  state = { ...state, ...next }
  listeners.forEach((listener) => listener())
}
export function launcherUpdateBusy(phase: Phase) {
  return ['checking', 'downloading', 'verifying', 'installing', 'restarting'].includes(phase)
}

export function checkLauncherUpdate(): Promise<void> {
  if (pending) return pending
  if (state.installed) return Promise.resolve()
  patch({ phase: 'checking', error: null, downloaded: 0, total: null })
  pending = (async () => {
    try {
      patch({ currentVersion: await getVersion() })
      const update = await invoke<LauncherUpdate | null>('check_launcher_update')
      patch({ update, phase: update ? 'available' : 'current', checkedAt: Date.now() })
    } catch (error) {
      patch({ phase: 'error', error: String(error) })
    }
  })().finally(() => {
    pending = null
  })
  return pending
}

export function installLauncherUpdate(): Promise<void> {
  if (pending) return pending
  if (!state.update || state.installed) return Promise.resolve()
  patch({ phase: 'downloading', downloaded: 0, total: null, error: null })
  pending = (async () => {
    let active = true
    const onProgress = new Channel<Progress>()
    onProgress.onmessage = (progress) => {
      if (active) patch(progress.phase === 'verifying' ? { phase: 'verifying' } : progress)
    }
    try {
      await invoke('install_launcher_update', { onProgress })
      patch({ phase: 'installed', installed: true })
    } catch (error) {
      patch({ phase: 'error', error: String(error) })
    } finally {
      active = false
    }
  })().finally(() => {
    pending = null
  })
  return pending
}

export async function restartLauncher() {
  if (!state.installed || pending || state.phase === 'restarting') return
  patch({ phase: 'restarting', error: null })
  try {
    const game = await invoke<{ phase: string }>('game_status')
    if (game.phase !== 'idle') throw new Error('Закройте игру перед перезапуском лаунчера')
    await relaunch()
  } catch (error) {
    patch({ phase: 'error', error: String(error) })
  }
}

export function launcherUpdateStatus(value: LauncherUpdateState): string {
  switch (value.phase) {
    case 'idle':
      return 'Проверка ещё не выполнялась'
    case 'checking':
      return 'Проверяем обновления…'
    case 'current':
      return 'Установлена актуальная версия'
    case 'available':
      return `Доступна версия ${value.update?.version}`
    case 'downloading':
      return 'Скачиваем обновление…'
    case 'verifying':
      return 'Проверяем подпись…'
    case 'installing':
      return 'Устанавливаем обновление…'
    case 'installed':
      return `Версия ${value.update?.version} установлена. Перезапустите лаунчер.`
    case 'restarting':
      return 'Перезапускаем лаунчер…'
    case 'error':
      return 'Не удалось завершить обновление'
  }
}
