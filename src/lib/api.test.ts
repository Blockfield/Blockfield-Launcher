import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchServerStatus, openExternalUrl } from './api'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const mockShellOpen = vi.fn()
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: (...args: unknown[]) => mockShellOpen(...args),
}))

afterEach(() => {
  invoke.mockReset()
  mockShellOpen.mockReset()
  vi.restoreAllMocks()
})

describe('fetchServerStatus', () => {
  it('keeps explicit offline telemetry unavailable', async () => {
    invoke.mockResolvedValue({
      online: false,
      playersOnline: null,
      playersMax: null,
      minecraftVersion: null,
      motd: null,
      host: 'play.example.test',
      port: 25565,
      regionCode: '',
      locationName: '',
      serverLatencyMs: null,
      checkedAt: 123,
    })

    const result = await fetchServerStatus()

    expect(invoke).toHaveBeenCalledWith('server_status')
    expect(result.online).toBe(false)
    expect(result.playersOnline).toBeNull()
    expect(result.serverLatencyMs).toBeNull()
    expect(result.apiLatencyMs).toBeGreaterThanOrEqual(0)
  })

  it('rejects when the backend cannot resolve the server', async () => {
    invoke.mockRejectedValue(new Error('VITE_BLOCKFIELD_PACK_URL is required'))

    await expect(fetchServerStatus()).rejects.toThrow('VITE_BLOCKFIELD_PACK_URL is required')
  })
})

describe('openExternalUrl', () => {
  it('opens URL via plugin-shell when available', async () => {
    mockShellOpen.mockResolvedValue(undefined)
    await openExternalUrl('https://github.com/netherg-io/blockfield-launcher-releases')
    expect(mockShellOpen).toHaveBeenCalledWith(
      'https://github.com/netherg-io/blockfield-launcher-releases',
    )
  })

  it('falls back to window.open if plugin-shell fails', async () => {
    mockShellOpen.mockRejectedValue(new Error('Shell plugin disabled'))
    const mockWindowOpen = vi.fn()
    vi.stubGlobal('window', { open: mockWindowOpen })

    try {
      await openExternalUrl('https://github.com/netherg-io/blockfield-launcher-releases')

      expect(mockWindowOpen).toHaveBeenCalledWith(
        'https://github.com/netherg-io/blockfield-launcher-releases',
        '_blank',
        'noopener,noreferrer',
      )
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
