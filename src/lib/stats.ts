export type StatsMode = 'room' | 'campaign' | 'deathmatch'
export type StatsPeriod = 'all' | 'week' | 'month'
export type StatsSort =
  | 'score'
  | 'kills'
  | 'captures'
  | 'pointSeconds'
  | 'playSeconds'
  | 'wins'
  | 'kd'
  | 'rating'

export interface StatsMmr {
  rating: number
  rd: number
  ratedCampaigns: number
  provisional: boolean
}

export interface StatsRow {
  uuid: string
  name: string
  score: number
  kills: number
  deaths: number
  captures: number
  neutralizations: number
  pointSeconds: number
  playSeconds: number
  suicides: number
  teamKills: number
  maxKillstreak: number
  matches: number
  wins: number
  kd: number
  mmr: StatsMmr | null
  rank?: number
}

export interface Leaderboard {
  schemaVersion: number
  checkedAt?: string
  mode?: string
  page: number
  limit: number
  total: number
  ratingStatus?: string
  players: StatsRow[]
}

export interface StatsSeason {
  id: string
  name: string
  startsAt: string | number
  endsAt: string | number | null
}

export interface StatsSeasons {
  current: string | null
  seasons: StatsSeason[]
}

export interface PlayerHistoryEntry {
  eventId: string
  kind: string
  mode: string
  endedAt: number
  reason: string
  winner: string
  winnerUuid: string | null
  team: string
  score: number
  kills: number
  deaths: number
}

export interface RatingHistoryEntry {
  campaignSessionId: string
  ratedAt: number
  team: string
  score: number
  before: number
  after: number
  rd: number
}

export interface PlayerStats {
  schemaVersion: number
  checkedAt?: string
  profile: { uuid: string; name: string; lastSeen: number; role?: string | null }
  totals: StatsRow | null
  classes: unknown[]
  campaigns: { completed: number; wins: number | null }
  history: PlayerHistoryEntry[]
  mmr: StatsMmr | null
  ratingHistory: RatingHistoryEntry[]
  ratingStatus?: string
}

export type StatsQuery = Partial<
  Record<'mode' | 'period' | 'season' | 'sort' | 'limit' | 'page', string>
>

export type StatsRequest =
  | { kind: 'leaderboard'; query: StatsQuery }
  | { kind: 'seasons' }
  | { kind: 'player'; uuid: string; query: StatsQuery }

/**
 * `data === null` without `error` means the API answered 404 (unknown player / no seasons).
 * `stale` marks cached data shown because the latest request failed.
 */
export interface StatsResult<T> {
  data: T | null
  savedAt: number | null
  stale: boolean
  error: string | null
}

function cacheKey(request: StatsRequest) {
  const query =
    'query' in request ? new URLSearchParams(Object.entries(request.query).sort()).toString() : ''
  return `bf.stats.${request.kind}${'uuid' in request ? `.${request.uuid}` : ''}${query ? `?${query}` : ''}`
}

function invokeStats(
  request: StatsRequest,
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
) {
  switch (request.kind) {
    case 'leaderboard':
      return invoke('stats_leaderboard', { query: request.query })
    case 'seasons':
      return invoke('stats_seasons')
    case 'player':
      return invoke('stats_player', { uuid: request.uuid.toLowerCase(), query: request.query })
  }
}

export async function loadStats<T>(request: StatsRequest): Promise<StatsResult<T>> {
  const key = cacheKey(request)
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const data = ((await invokeStats(request, invoke)) ?? null) as T | null
    const savedAt = Date.now()
    try {
      localStorage.setItem(key, JSON.stringify({ savedAt, data }))
    } catch {
      // Storage full or unavailable: fresh data is still shown.
    }
    return { data, savedAt, stale: false, error: null }
  } catch (e) {
    const error = String(e)
    try {
      const cached = JSON.parse(localStorage.getItem(key) ?? 'null') as {
        savedAt: number
        data: T | null
      } | null
      if (cached && typeof cached.savedAt === 'number') {
        return { data: cached.data, savedAt: cached.savedAt, stale: true, error }
      }
    } catch {
      // Unreadable cache is the same as no cache.
    }
    return { data: null, savedAt: null, stale: false, error }
  }
}

export const formatMmr = (mmr: StatsMmr | null | undefined) =>
  mmr ? `${Math.round(mmr.rating)}${mmr.provisional ? '?' : ''}` : '—'

export const formatMmrRange = (mmr: StatsMmr | null | undefined) =>
  mmr ? `${formatMmr(mmr)} ± ${Math.round(2 * mmr.rd)}` : '—'

export const formatKd = (row: Pick<StatsRow, 'kills' | 'deaths'>) => `${row.kills} / ${row.deaths}`

export const formatScore = (score: number) => String(Math.round(score * 100) / 100)

export const formatBattles = (row: Pick<StatsRow, 'matches' | 'wins'>) =>
  `${row.matches} / ${row.wins}`

export const displayRank = (
  row: Pick<StatsRow, 'rank'>,
  index: number,
  page: number,
  limit: number,
) => row.rank ?? (page - 1) * limit + index + 1
