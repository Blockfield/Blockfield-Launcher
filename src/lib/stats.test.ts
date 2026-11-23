import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  displayRank,
  formatBattles,
  formatKd,
  formatMmr,
  formatMmrRange,
  formatScore,
  isPlayerUuid,
  loadStats,
  playerProfileUrl,
} from './stats'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const request = { kind: 'leaderboard', query: { mode: 'room', page: '1' } } as const

describe('loadStats', () => {
  beforeEach(() => {
    const store = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => store.set(key, value),
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.mocked(invoke).mockReset()
  })

  it('returns cached data flagged stale when the API fails', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ total: 1, players: [{ name: 'NetherG' }] })
    const fresh = await loadStats(request)
    expect(fresh.stale).toBe(false)
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('stats_leaderboard', { query: request.query })

    vi.mocked(invoke).mockRejectedValueOnce('Stats API unavailable: timeout')
    const stale = await loadStats(request)
    expect(stale).toEqual({
      data: { total: 1, players: [{ name: 'NetherG' }] },
      savedAt: fresh.savedAt,
      stale: true,
      error: 'Stats API unavailable: timeout',
    })
  })

  it('returns an error without data when nothing is cached', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('Stats API: Unknown mode')
    expect(await loadStats(request)).toEqual({
      data: null,
      savedAt: null,
      stale: false,
      error: 'Stats API: Unknown mode',
    })
  })

  it('treats 404 as a fresh empty answer and lowercases player uuids', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(null)
    const result = await loadStats({
      kind: 'player',
      uuid: 'F3E1B9A6-2270-3DEE-8782-B7CDD8519EFE',
      query: {},
    })
    expect(result).toMatchObject({ data: null, stale: false, error: null })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('stats_player', {
      uuid: 'f3e1b9a6-2270-3dee-8782-b7cdd8519efe',
      query: {},
    })
  })
})

describe('playerProfileUrl', () => {
  const uuid = 'F3E1B9A6-2270-3DEE-8782-B7CDD8519EFE'

  it('builds the site player URL with a lowercased uuid', () => {
    expect(playerProfileUrl('https://blockfield.pro', uuid)).toBe(
      'https://blockfield.pro/players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe',
    )
  })

  it('rejects anything that is not a dashed uuid', () => {
    for (const bad of [
      '',
      'NetherG',
      'f3e1b9a6_2270_3dee_8782_b7cdd8519efe',
      `${uuid}-x`,
      'https://blockfield.pro/players/f3e1b9a6-2270-3dee-8782-b7cdd8519efe',
    ]) {
      expect(playerProfileUrl('https://blockfield.pro', bad)).toBeNull()
      expect(isPlayerUuid(bad)).toBe(false)
    }
    expect(isPlayerUuid(uuid)).toBe(true)
  })
})

describe('stats formatting', () => {
  it('formats MMR, K/D, score, battles and rank', () => {
    expect(formatMmr({ rating: 1523.6, rd: 80, ratedCampaigns: 2, provisional: true })).toBe(
      '1524?',
    )
    expect(formatMmr({ rating: 1500, rd: 50, ratedCampaigns: 10, provisional: false })).toBe('1500')
    expect(formatMmr(null)).toBe('—')
    expect(formatMmrRange({ rating: 1500, rd: 50.4, ratedCampaigns: 10, provisional: false })).toBe(
      '1500 ± 101',
    )
    expect(formatKd({ kills: 12, deaths: 0 })).toBe('12 / 0')
    expect(formatScore(12.3456)).toBe('12.35')
    expect(formatScore(7)).toBe('7')
    expect(formatBattles({ matches: 9, wins: 4 })).toBe('9 / 4')
    expect(displayRank({}, 2, 3, 25)).toBe(53)
    expect(displayRank({ rank: 7 }, 2, 3, 25)).toBe(7)
  })
})
