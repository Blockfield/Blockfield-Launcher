import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchServerStatus } from './api'

afterEach(() => vi.unstubAllGlobals())

describe('fetchServerStatus', () => {
  it('keeps explicit offline telemetry unavailable', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          online: false,
          playersOnline: null,
          playersMax: null,
          minecraftVersion: null,
          motd: null,
          host: 'play.example.test',
          port: 25565,
          regionCode: 'TEST',
          locationName: 'Test region',
          serverLatencyMs: null,
          checkedAt: 123,
        }),
      }),
    )

    const result = await fetchServerStatus()

    expect(result.online).toBe(false)
    expect(result.playersOnline).toBeNull()
    expect(result.playersMax).toBeNull()
    expect(result.serverLatencyMs).toBeNull()
    expect(result.apiLatencyMs).toBeGreaterThanOrEqual(0)
  })

  it('rejects an unavailable status API instead of inventing values', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 503 }))

    await expect(fetchServerStatus()).rejects.toThrow('Server status failed: 503')
  })
})
