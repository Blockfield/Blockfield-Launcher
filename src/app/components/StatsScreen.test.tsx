import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { StatsView } from './StatsScreen'
import type { Leaderboard, PlayerStats, StatsResult, StatsRow } from '../../lib/stats'

const ME = 'f3e1b9a6-2270-3dee-8782-b7cdd8519efe'

const row = (uuid: string, name: string): StatsRow => ({
  uuid,
  name,
  score: 12.345,
  kills: 7,
  deaths: 3,
  captures: 2,
  neutralizations: 0,
  pointSeconds: 10,
  playSeconds: 300,
  suicides: 0,
  teamKills: 0,
  maxKillstreak: 2,
  matches: 4,
  wins: 1,
  kd: 2.33,
  mmr: { rating: 1512, rd: 90, ratedCampaigns: 1, provisional: true },
})

const fresh = <T,>(data: T | null): StatsResult<T> => ({
  data,
  savedAt: 1,
  stale: false,
  error: null,
})

const board = (players: StatsRow[]): Leaderboard => ({
  schemaVersion: 1,
  page: 2,
  limit: 25,
  total: 30,
  players,
})

function render(props: Partial<Parameters<typeof StatsView>[0]>) {
  return renderToStaticMarkup(
    <StatsView
      uuid={ME}
      filters={{ mode: '', period: 'all', sort: 'score', page: 1 }}
      onFilters={() => {}}
      onRefresh={() => {}}
      board={null}
      self={null}
      seasons={null}
      {...props}
    />,
  )
}

describe('StatsView', () => {
  it('shows loading placeholders while requests are pending', () => {
    const html = render({})
    expect(html.match(/Загрузка статистики/g)).toHaveLength(2)
    expect(html).toContain('Casual')
    expect(html).not.toContain('<table')
  })

  it('shows the empty period message instead of zero rows', () => {
    const html = render({ board: fresh({ ...board([]), total: 0 }) })
    expect(html).toContain('За выбранный период ещё нет результатов')
    expect(html).not.toContain('<table')
  })

  it('tells a player without recorded games that they have not played yet', () => {
    const html = render({ board: fresh(board([])), self: fresh<PlayerStats>(null) })
    expect(html).toContain('Вы ещё не играли')
  })

  it('marks cached data as stale with its timestamp', () => {
    const html = render({
      board: {
        data: board([row(ME, 'NetherG')]),
        savedAt: Date.UTC(2026, 8, 13),
        stale: true,
        error: 'timeout',
      },
    })
    expect(html).toContain('Статистика недоступна · данные на')
    expect(html).toContain('NetherG')
  })

  it('shows an error without inventing a leaderboard', () => {
    const html = render({
      board: { data: null, savedAt: null, stale: false, error: 'Stats API unavailable' },
      self: { data: null, savedAt: null, stale: false, error: 'Stats API unavailable' },
    })
    expect(html).toContain('Статистика недоступна, попробуйте позже.')
    expect(html).toContain('Ваша статистика недоступна.')
    expect(html).not.toContain('Вы ещё не играли')
    expect(html).not.toContain('<table')
  })

  it('highlights the own row and computes ranks from the page when missing', () => {
    const other = '00000000-0000-0000-0000-000000000001'
    const html = render({ board: fresh(board([row(other, 'Other'), row(ME, 'NetherG')])) })
    expect(html.match(/aria-current="true"/g)).toHaveLength(1)
    expect(html).toMatch(/aria-current="true"[^>]*><td[^>]*>27<\/td>/)
    expect(html).toContain('1512?')
    expect(html).toContain('7 / 3')
    expect(html).toContain('12.35')
    expect(html).toContain('4 / 1')
  })

  it('offers seasons only when the API lists them', () => {
    expect(render({})).not.toContain('Сезон 1')
    const html = render({
      seasons: {
        current: 's1',
        seasons: [{ id: 's1', name: 'Сезон 1', startsAt: 0, endsAt: null }],
      },
    })
    expect(html).toContain('Сезон 1')
  })
})
