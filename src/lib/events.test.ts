import { beforeEach, expect, it, vi } from 'vitest'
import type { LauncherStatus, DownloadProgress } from './api'

const { listen, nativeUnlisten, handlers } = vi.hoisted(() => ({
  listen: vi.fn(),
  nativeUnlisten: vi.fn(),
  handlers: [] as Array<(event: { payload: unknown }) => void>,
}))
vi.mock('@tauri-apps/api/event', () => ({ listen }))

beforeEach(() => {
  handlers.length = 0
  nativeUnlisten.mockReset()
  listen.mockImplementation(async (_event, handler) => {
    handlers.push(handler)
    return nativeUnlisten
  })
})

it('ignores queued installer statuses after completion even before native unsubscription finishes', async () => {
  const { listenLauncherStatus } = await import('./events')
  const callback = vi.fn()
  const stop = await listenLauncherStatus(callback)
  const status: LauncherStatus = { phase: 'modpack', message: 'Checking files', cancelable: true }
  handlers[0]!({ payload: status })
  stop()
  handlers[0]!({ payload: { ...status, message: 'Finished successfully!' } })
  expect(callback).toHaveBeenCalledExactlyOnceWith(status)
  expect(nativeUnlisten).toHaveBeenCalledOnce()

  const nextCallback = vi.fn()
  await listenLauncherStatus(nextCallback)
  handlers[1]!({ payload: status })
  expect(nextCallback).toHaveBeenCalledExactlyOnceWith(status)
})

it('ignores queued progress from a completed operation', async () => {
  const { listenDownloadProgress } = await import('./events')
  const callback = vi.fn()
  const stop = await listenDownloadProgress(callback)
  stop()
  const progress = { totalBytesDownloaded: 1, totalBytesAll: 2 } as DownloadProgress
  handlers[0]!({ payload: progress })
  expect(callback).not.toHaveBeenCalled()
})
