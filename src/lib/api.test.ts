import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchServerStatus } from './api'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

afterEach(() => invoke.mockReset())

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
