import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchServerStatus, openExternalUrl, openPlayerProfile } from './api'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

afterEach(() => {
  invoke.mockReset()
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
  it('opens URL through the launcher backend when available', async () => {
    invoke.mockResolvedValue(undefined)
    await openExternalUrl('https://github.com/Blockfield/Blockfield-Launcher')
    expect(invoke).toHaveBeenCalledWith('open_url', {
      url: 'https://github.com/Blockfield/Blockfield-Launcher',
    })
  })

  it('falls back to window.open if the backend cannot open it', async () => {
    invoke.mockRejectedValue(new Error('Cannot open link'))
    const mockWindowOpen = vi.fn()
    vi.stubGlobal('window', { open: mockWindowOpen })

    try {
      await openExternalUrl('https://github.com/Blockfield/Blockfield-Launcher')

      expect(mockWindowOpen).toHaveBeenCalledWith(
        'https://github.com/Blockfield/Blockfield-Launcher',
        '_blank',
        'noopener,noreferrer',
      )
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

describe('openPlayerProfile', () => {
  it('opens the site player page through the allow-listed backend command', async () => {
    invoke.mockResolvedValue(undefined)
    await openPlayerProfile('F3E1B9A6-2270-3DEE-8782-B7CDD8519EFE')
    expect(invoke).toHaveBeenCalledWith('open_player_profile', {
      uuid: 'f3e1b9a6-2270-3dee-8782-b7cdd8519efe',
    })
  })
})
