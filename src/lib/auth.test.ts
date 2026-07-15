import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { login, logout, register, restoreSession } from './auth'

function memoryStorage(): Storage {
  const values = new Map<string, string>()
  return {
    get length() {
      return values.size
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => void values.delete(key),
    setItem: (key, value) => void values.set(key, value),
  }
}

const response = (ok: boolean, body: unknown = {}, status = ok ? 200 : 401) => ({
  ok,
  status,
  json: async () => body,
})

const sessionBody = {
  accessToken: 'access-token',
  refreshToken: 'refresh-token',
  expiresIn: 900,
  user: {
    id: 1,
    username: 'operator',
    email: null,
    minecraft_uuid: '00000000-0000-4000-8000-000000000001',
    role: 'player',
  },
}

beforeEach(() => {
  vi.stubGlobal('localStorage', memoryStorage())
  vi.stubGlobal('sessionStorage', memoryStorage())
  vi.stubGlobal('navigator', { userAgent: 'vitest' })
})

afterEach(() => vi.unstubAllGlobals())

describe('launcher auth session', () => {
  it('persists remembered login and restores it only after auth/me succeeds', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(response(true, sessionBody))
      .mockResolvedValueOnce(response(true))
    vi.stubGlobal('fetch', fetchMock)

    const session = await login('operator', 'password', true)
    expect(localStorage.getItem('blockfield.auth')).toContain('refresh-token')
    expect(sessionStorage.getItem('blockfield.auth')).toBeNull()
    await expect(restoreSession()).resolves.toEqual(session)
  })

  it('clears a revoked session when refresh fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(response(true, sessionBody))
        .mockResolvedValueOnce(response(false))
        .mockResolvedValueOnce(response(false)),
    )
    await login('operator', 'password', false)

    await expect(restoreSession()).resolves.toBeNull()
    expect(sessionStorage.getItem('blockfield.auth')).toBeNull()
  })

  it('clears local state before remote logout', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response(true, sessionBody)))
    const session = await login('operator', 'password', true)

    await logout(session)

    expect(localStorage.getItem('blockfield.auth')).toBeNull()
  })

  it('registers an account and persists the returned session', async () => {
    const fetchMock = vi.fn().mockResolvedValue(response(true, sessionBody, 201))
    vi.stubGlobal('fetch', fetchMock)

    await register('operator', 'operator@example.com', 'password', 'password', true)

    expect(localStorage.getItem('blockfield.auth')).toContain('access-token')
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('/auth/register'),
      expect.objectContaining({ method: 'POST' }),
    )
  })
})
