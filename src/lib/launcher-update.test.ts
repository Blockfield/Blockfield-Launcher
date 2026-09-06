import { beforeEach, expect, it, vi } from 'vitest'

const { invoke, getVersion, relaunch, channels } = vi.hoisted(() => ({
  invoke: vi.fn(),
  getVersion: vi.fn(),
  relaunch: vi.fn(),
  channels: [] as Array<{ onmessage: (event: unknown) => void }>,
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage = (event: unknown) => {
      void event
    }
    constructor() {
      channels.push(this)
    }
  },
}))
vi.mock('@tauri-apps/api/app', () => ({ getVersion }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  channels.length = 0
  getVersion.mockResolvedValue('0.2.5')
  invoke.mockImplementation(async (command: string) =>
    command === 'game_status'
      ? { phase: 'idle' }
      : { currentVersion: '0.2.5', version: '0.2.6', body: 'Fixes' },
  )
})

it('shares automatic and manual checks and waits for explicit installation', async () => {
  const api = await import('./launcher-update')
  await Promise.all([api.checkLauncherUpdate(), api.checkLauncherUpdate()])
  expect(invoke).toHaveBeenCalledExactlyOnceWith('check_launcher_update')
  expect(api.getLauncherUpdate()).toMatchObject({ phase: 'available', currentVersion: '0.2.5' })
  expect(relaunch).not.toHaveBeenCalled()
})

it('shows an up-to-date result with no available update', async () => {
  invoke.mockResolvedValue(null)
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  expect(api.getLauncherUpdate()).toMatchObject({
    phase: 'current',
    update: null,
    checkedAt: expect.any(Number),
  })
})

it('reports failed checks and permits retry', async () => {
  invoke.mockRejectedValueOnce('Network unavailable')
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  expect(api.getLauncherUpdate()).toMatchObject({ phase: 'error', error: 'Network unavailable' })
  await api.checkLauncherUpdate()
  expect(api.getLauncherUpdate()).toMatchObject({ phase: 'available', error: null })
})

it('shows bytes without a total, verification, installation and completion; ignores late progress', async () => {
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  let finish!: () => void
  invoke.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve
      }),
  )
  const pending = api.installLauncherUpdate()
  const duplicate = api.installLauncherUpdate()
  expect(duplicate).toBe(pending)
  channels[0]!.onmessage({ phase: 'downloading', downloaded: 1048576, total: null })
  expect(api.getLauncherUpdate()).toMatchObject({
    phase: 'downloading',
    downloaded: 1048576,
    total: null,
  })
  channels[0]!.onmessage({ phase: 'verifying', downloaded: 0, total: null })
  expect(api.getLauncherUpdate()).toMatchObject({ phase: 'verifying', downloaded: 1048576 })
  channels[0]!.onmessage({ phase: 'installing', downloaded: 2097152, total: 2097152 })
  expect(api.getLauncherUpdate().phase).toBe('installing')
  finish()
  await pending
  channels[0]!.onmessage({ phase: 'downloading', downloaded: 1, total: 20 })
  expect(api.getLauncherUpdate()).toMatchObject({ phase: 'installed', installed: true })
  expect(relaunch).not.toHaveBeenCalled()
  await api.checkLauncherUpdate()
  await api.installLauncherUpdate()
  expect(invoke).toHaveBeenCalledTimes(2)
  invoke.mockResolvedValueOnce({ phase: 'idle' })
  await api.restartLauncher()
  expect(relaunch).toHaveBeenCalledOnce()
})

it('retains installation errors and never restarts after a rejected signature', async () => {
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  invoke.mockRejectedValueOnce('Invalid signature')
  await api.installLauncherUpdate()
  channels[0]!.onmessage({ phase: 'installing', downloaded: 2, total: 2 })
  expect(api.getLauncherUpdate()).toMatchObject({
    phase: 'error',
    installed: false,
    error: 'Invalid signature',
  })
  await api.restartLauncher()
  expect(relaunch).not.toHaveBeenCalled()
  invoke.mockResolvedValueOnce(undefined)
  await api.installLauncherUpdate()
  expect(api.getLauncherUpdate().phase).toBe('installed')
})

it('preserves installed state when restarting fails so restart can be retried', async () => {
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  await api.installLauncherUpdate()
  relaunch.mockRejectedValueOnce('Restart failed')
  await api.restartLauncher()
  expect(api.getLauncherUpdate()).toMatchObject({
    phase: 'error',
    installed: true,
    error: 'Restart failed',
  })
  await api.restartLauncher()
  expect(relaunch).toHaveBeenCalledTimes(2)
})

it('does not restart the launcher while the game is active', async () => {
  const api = await import('./launcher-update')
  await api.checkLauncherUpdate()
  await api.installLauncherUpdate()
  invoke.mockResolvedValueOnce({ phase: 'running' })
  await api.restartLauncher()
  expect(relaunch).not.toHaveBeenCalled()
  expect(api.getLauncherUpdate()).toMatchObject({ installed: true, phase: 'error' })
})
