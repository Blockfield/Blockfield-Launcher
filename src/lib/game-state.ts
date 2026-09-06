import { useSyncExternalStore } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export interface GameStatus {
  phase: 'idle' | 'launching' | 'running' | 'finishing'
  revision: number
}
let state: Omit<GameStatus, 'phase'> & { phase: GameStatus['phase'] | 'unknown' } = {
  phase: 'unknown',
  revision: -1,
}
const listeners = new Set<() => void>()
let launchRequest: Promise<void> | null = null
export const getGameState = () => state
export const useGameState = () =>
  useSyncExternalStore((listener) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }, getGameState)
function apply(status: GameStatus) {
  if (status.revision < state.revision) return
  state = status
  listeners.forEach((listener) => listener())
}
export async function watchGameState() {
  let active = true
  const stop = await listen<GameStatus>('game://status', ({ payload }) => {
    if (active) apply(payload)
  })
  try {
    const status = await invoke<GameStatus>('game_status')
    if (active) apply(status)
  } catch (error) {
    active = false
    stop()
    throw error
  }
  return () => {
    active = false
    stop()
  }
}
export function launchGame(): Promise<void> {
  if (launchRequest) return launchRequest
  if (state.phase !== 'idle')
    return Promise.reject(new Error('Игра уже запущена или готовится к запуску'))
  launchRequest = (async () => {
    try {
      await invoke('launch_game')
    } finally {
      try {
        apply(await invoke<GameStatus>('game_status'))
      } catch (error) {
        console.error('Failed to refresh game state:', error)
      }
    }
  })().finally(() => {
    launchRequest = null
  })
  return launchRequest
}
export function gameStateLabel(phase: ReturnType<typeof getGameState>['phase']) {
  switch (phase) {
    case 'running':
      return 'В ИГРЕ'
    case 'launching':
      return 'ЗАПУСК'
    case 'finishing':
      return 'ЗАВЕРШЕНИЕ'
    case 'unknown':
      return 'ПРОВЕРКА…'
    case 'idle':
      return 'ИГРАТЬ'
  }
}
