// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { MainScreen } from './MainScreen'
import type { ServerStatus, VersionCheckResult } from '../../lib/api'

const { invoke, checkModpack, useModpackVersion } = vi.hoisted(() => ({
  invoke: vi.fn(),
  checkModpack: vi.fn(),
  useModpackVersion: vi.fn(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('../../lib/modpack-check', () => ({
  checkModpack,
  useModpackVersion,
  invalidateModpackCheck: vi.fn(),
}))
vi.mock('../../lib/game-state', () => ({
  useGameState: () => ({ phase: 'idle', revision: 0 }),
  gameStateLabel: (phase: string) => phase,
  launchGame: vi.fn(async () => {}),
}))
vi.mock('../../lib/events', () => ({
  listenDownloadProgress: vi.fn(async () => () => {}),
  listenDownloadMirror: vi.fn(async () => () => {}),
  listenLauncherStatus: vi.fn(async () => () => {}),
}))

const staleVersion: VersionCheckResult = {
  needsUpdate: true,
  remoteVersion: '2.80.0',
  installedVersion: '2.79.0',
  mirror: 'https://example.invalid',
  fileCount: 0,
  totalSize: 0,
  javaOk: true,
  loaderOk: true,
}

const onlineStatus: ServerStatus = {
  rooms: [
    {
      id: 'first',
      name: 'Матч 1',
      mode: 'Захват точек',
      map: 'Город',
      phase: 'IDLE',
      players: 2,
      joinable: true,
    },
  ],
  gameAvailable: true,
  maintenance: null,
  online: true,
  playersOnline: 2,
  playersMax: 200,
  minecraftVersion: '1.21.1',
  motd: null,
  host: 'play.example',
  port: 25565,
  regionCode: 'DE',
  locationName: '',
  serverLatencyMs: 12,
  checkedAt: 0,
  apiLatencyMs: 1,
}

let container: HTMLDivElement
let root: Root | null

beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  // MainScreen only runs its version check inside Tauri; the flag lets the
  // mocked checkModpack resolve so the main button becomes enabled.
  ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
  container = document.createElement('div')
  document.body.append(container)
  root = createRoot(container)
  checkModpack.mockResolvedValue(staleVersion)
  useModpackVersion.mockReturnValue(staleVersion)
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'server_status') return onlineStatus
    return null
  })
})

afterEach(async () => {
  await act(async () => root?.unmount())
  root = null
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  container.remove()
})

function updateButtons(): HTMLButtonElement[] {
  return Array.from(container.querySelectorAll('button')).filter((button) =>
    button.textContent?.includes('ОБНОВИТЬ'),
  )
}

it('routes ОБНОВИТЬ to a sync without arming auto-launch (launcher#156)', async () => {
  const onPlay = vi.fn()
  const onUpdate = vi.fn()
  await act(async () => {
    root!.render(<MainScreen onPlay={onPlay} onUpdate={onUpdate} profile="game" />)
  })
  await act(async () => {})
  const buttons = updateButtons()
  expect(buttons).toHaveLength(1)
  expect(buttons[0]!.disabled).toBe(false)
  await act(async () => {
    buttons[0]!.click()
  })
  expect(onUpdate).toHaveBeenCalledTimes(1)
  expect(onPlay).not.toHaveBeenCalled()
})

it('keeps Play intent for room invites so the game launches after the sync', async () => {
  const onPlay = vi.fn()
  const onUpdate = vi.fn()
  await act(async () => {
    root!.render(<MainScreen onPlay={onPlay} onUpdate={onUpdate} profile="game" />)
  })
  await act(async () => {})
  const join = container.querySelector<HTMLButtonElement>('[aria-label="Войти: Матч 1"]')
  expect(join).not.toBeNull()
  await act(async () => {
    join!.click()
  })
  expect(onPlay).toHaveBeenCalledTimes(1)
  expect(onUpdate).not.toHaveBeenCalled()
})
