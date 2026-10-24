// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const native = vi.fn()
const callback = vi.fn()

beforeEach(() => {
  vi.resetModules()
  native.mockReset()
  callback.mockReset()
  vi.stubGlobal('Date', Date)
  vi.stubGlobal('fetch', vi.fn())
  vi.spyOn(console, 'error').mockImplementation(() => {})
  vi.spyOn(window, 'open').mockImplementation(() => null)
  vi.spyOn(performance, 'now').mockReturnValue(1000)
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke: native, runCallback: callback })
  document.body.innerHTML = '<div id="root"><span>Existing UI</span></div>'
})

afterEach(() => {
  document.head.querySelectorAll('style').forEach((node) => node.remove())
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

it('intercepts application IPC and HTTP, retains native window IPC, and emits 42% progress', async () => {
  const { config, FIXED_TIME } = await import('./data')
  native.mockResolvedValue({
    ...config,
    username: JSON.stringify({ state: 'update-42', lang: 'en', scale: 1, motion: false }),
  })
  const { install } = await import('./index')
  await install()
  const { invoke } = await import('@tauri-apps/api/core')
  expect(Date.now()).toBe(FIXED_TIME)
  expect(new Date().toISOString()).toBe('2026-10-24T12:00:00.000Z')
  await expect(invoke('account_status')).resolves.toMatchObject({ loggedIn: true })
  await invoke('plugin:event|listen', { handler: 42, event: 'download://progress' })
  void invoke('download_modpack')
  expect(callback).toHaveBeenCalledWith(
    42,
    expect.objectContaining({
      payload: expect.objectContaining({ totalBytesDownloaded: 420_000_000 }),
    }),
  )
  await expect(invoke('launch_game')).rejects.toThrow('unexpected IPC')
  await invoke('plugin:window|close')
  expect(native).toHaveBeenCalledWith('plugin:window|close', {})
  expect(await (await fetch('https://capture.invalid/content.json')).json()).toEqual({})
  await expect(fetch('https://blockfield.pro/real-data')).rejects.toThrow('unexpected HTTP')
})

it('rejects a capture when Inter has no loaded FontFace, even if fonts.check returns true', async () => {
  const { config } = await import('./data')
  native.mockImplementation(async (command) =>
    command === 'load_settings'
      ? {
          ...config,
          username: JSON.stringify({ state: 'main', lang: 'en', scale: 1, motion: false }),
        }
      : null,
  )
  vi.stubGlobal('FontFaceSet', undefined)
  Object.defineProperty(document, 'fonts', {
    configurable: true,
    value: {
      load: async () => [],
      ready: Promise.resolve(),
      check: () => true,
      [Symbol.iterator]: function* () {
        yield { family: 'Inter', weight: '400', status: 'unloaded' }
      },
    },
  })
  const { install, drive } = await import('./index')
  await install()
  await drive()
  expect(native).toHaveBeenLastCalledWith('save_settings', {
    config: expect.objectContaining({
      lang: expect.stringContaining('capture-failed:Error: Inter font was not loaded'),
    }),
  })
  Reflect.deleteProperty(document, 'fonts')
})
