import { useEffect, useState, type ReactNode } from 'react'
import { ExternalLink, LoaderCircle, RefreshCw } from 'lucide-react'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { PlayerHead } from './PlayerHead'
import { ErrorDetail, SectionHeader } from './ui-bits'
import { useI18n } from '../i18n'
import { openExternalUrl } from '../../lib/api'
import { apiBaseUrl } from '../../lib/api-base'
import {
  displayRank,
  formatBattles,
  formatKd,
  formatMmr,
  formatMmrRange,
  formatScore,
  loadStats,
  type Leaderboard,
  type PlayerStats,
  type StatsQuery,
  type StatsResult,
  type StatsSeasons,
} from '../../lib/stats'

const PAGE_SIZE = 25
const RECENT_MATCHES_SHOWN = 4

const MODES = [
  ['', 'Все'],
  ['room', 'Casual'],
  ['campaign', 'Ranked'],
  ['deathmatch', 'Deathmatch'],
] as const

const PERIODS = [
  ['all', 'Всё время'],
  ['week', '7 дней'],
  ['month', '30 дней'],
] as const

const SORTS = [
  ['score', 'Очки'],
  ['rating', 'MMR'],
  ['kd', 'K/D'],
  ['captures', 'Захваты'],
] as const

const MODE_LABELS: Record<string, string> = Object.fromEntries(MODES.filter(([id]) => id))

export interface StatsFilters {
  mode: string
  /** `all` | `week` | `month`, or `season:<id>`. */
  period: string
  sort: string
  page: number
}

type Tagged<T> = { key: string; result: StatsResult<T> }

const DEFAULT_FILTERS: StatsFilters = { mode: '', period: 'all', sort: 'score', page: 1 }

function filterQuery({ mode, period }: Pick<StatsFilters, 'mode' | 'period'>): StatsQuery {
  const season = period.startsWith('season:') ? period.slice('season:'.length) : null
  return {
    ...(mode ? { mode } : {}),
    ...(season ? { season } : { period }),
  }
}

const formatTime = (ms: number) =>
  new Date(ms).toLocaleString('ru-RU', { dateStyle: 'short', timeStyle: 'short' })

/** Loads stats on open, on filter change and on manual refresh. Never touches play/update state. */
export function StatsScreen({ uuid }: { uuid: string }) {
  const [filters, setFilters] = useState(DEFAULT_FILTERS)
  const [refreshCount, setRefreshCount] = useState(0)
  const [board, setBoard] = useState<Tagged<Leaderboard> | null>(null)
  const [self, setSelf] = useState<Tagged<PlayerStats> | null>(null)
  const [seasons, setSeasons] = useState<StatsSeasons | null>(null)
  // A result belongs to one filter set + refresh; anything else is shown as loading.
  const key = `${JSON.stringify(filters)}#${refreshCount}`

  useEffect(() => {
    let active = true
    const query = filterQuery(filters)
    void loadStats<Leaderboard>({
      kind: 'leaderboard',
      query: { ...query, sort: filters.sort, limit: String(PAGE_SIZE), page: String(filters.page) },
    }).then((result) => active && setBoard({ key, result }))
    void loadStats<PlayerStats>({ kind: 'player', uuid, query }).then(
      (result) => active && setSelf({ key, result }),
    )
    return () => {
      active = false
    }
  }, [filters, uuid, key])

  useEffect(() => {
    void loadStats<StatsSeasons>({ kind: 'seasons' }).then((result) => setSeasons(result.data))
  }, [])

  return (
    <StatsView
      uuid={uuid}
      filters={filters}
      onFilters={(next) => setFilters((current) => ({ ...current, page: 1, ...next }))}
      onRefresh={() => setRefreshCount((count) => count + 1)}
      board={board?.key === key ? board.result : null}
      self={self?.key === key ? self.result : null}
      seasons={seasons}
    />
  )
}

export function StatsView({
  uuid,
  filters,
  onFilters,
  onRefresh,
  board,
  self,
  seasons,
}: {
  uuid: string
  filters: StatsFilters
  onFilters: (next: Partial<StatsFilters>) => void
  onRefresh: () => void
  board: StatsResult<Leaderboard> | null
  self: StatsResult<PlayerStats> | null
  seasons: StatsSeasons | null
}) {
  const { t } = useI18n()
  const periods: Array<readonly [string, string]> = [
    ...PERIODS,
    ...(seasons?.seasons ?? []).map((season) => [`season:${season.id}`, season.name] as const),
  ]
  const data = board?.data
  const page = data?.page ?? filters.page
  const limit = data?.limit ?? PAGE_SIZE

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />
      <div className="screen-layout relative h-full min-h-0 flex flex-col gap-3 overflow-hidden">
        <div className="shrink-0 flex flex-wrap items-baseline gap-3">
          <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
            {t('nav.stats')}
          </h1>
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">ТАБЛИЦА ЛИДЕРОВ</span>
          {board?.stale && board.savedAt && (
            <span role="status" className="text-[11px] text-[#F5A524]">
              Статистика недоступна · данные на {formatTime(board.savedAt)}
            </span>
          )}
        </div>

        <div className="shrink-0 flex flex-wrap items-end gap-x-6 gap-y-2 border-b border-[#2A2116]">
          <FilterGroup
            label="Режим"
            options={MODES}
            value={filters.mode}
            onChange={(mode) => onFilters({ mode })}
          />
          <FilterGroup
            label="Период"
            options={periods}
            value={filters.period}
            onChange={(period) => onFilters({ period })}
          />
          <FilterGroup
            label="Сортировка"
            options={SORTS}
            value={filters.sort}
            onChange={(sort) => onFilters({ sort })}
          />
          <button
            type="button"
            onClick={onRefresh}
            disabled={!board}
            aria-label="Обновить статистику"
            className="ml-auto mb-1 size-8 grid place-items-center border border-[#2A2116] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] disabled:opacity-50 transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524]"
          >
            <RefreshCw size={13} className={board ? '' : 'animate-spin'} />
          </button>
        </div>

        <div className="min-h-0 flex-1 grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_300px] gap-px bg-[#18130D] border border-[#2A2116] overflow-y-auto md:overflow-hidden">
          <section className="bg-[#0B0906] min-h-0 flex flex-col">
            <div className="min-h-0 flex-1 overflow-auto">
              {!board ? (
                <Loading />
              ) : !data ? (
                <div className="p-4">
                  <ErrorDetail
                    message="Статистика недоступна, попробуйте позже."
                    raw={board.error}
                  />
                </div>
              ) : data.players.length === 0 ? (
                <Empty>За выбранный период ещё нет результатов</Empty>
              ) : (
                <table className="w-full text-[12px] text-[#C7AE86]">
                  <thead className="sticky top-0 bg-[#0B0906] text-[10px] tracking-[0.14em] text-[#8E7A5E]">
                    <tr className="border-b border-[#2A2116] text-right [&>th]:px-3 [&>th]:py-2 [&>th]:font-normal">
                      <th className="w-12 text-left">Место</th>
                      <th className="text-left">Игрок</th>
                      <th>MMR</th>
                      <th>Очки</th>
                      <th>K/D</th>
                      <th>Захваты</th>
                      <th>Бои / победы</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.players.map((row, index) => {
                      const own = row.uuid === uuid.toLowerCase()
                      return (
                        <tr
                          key={row.uuid}
                          aria-current={own ? 'true' : undefined}
                          className={`border-b border-[#18130D] text-right tabular-nums [&>td]:px-3 [&>td]:py-1.5 ${own ? 'bg-[#F5A524]/10 text-[#F3E7D0] shadow-[inset_2px_0_0_#F5A524]' : ''}`}
                        >
                          <td className="text-left text-[#8E7A5E]">
                            {displayRank(row, index, page, limit)}
                          </td>
                          <td className="text-left">
                            <span className="flex min-w-0 items-center gap-2 text-neutral-100">
                              <PlayerHead username={row.name} />
                              <span className="truncate">{row.name}</span>
                            </span>
                          </td>
                          <td>{formatMmr(row.mmr)}</td>
                          <td>{formatScore(row.score)}</td>
                          <td>{formatKd(row)}</td>
                          <td>{row.captures}</td>
                          <td>{formatBattles(row)}</td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              )}
            </div>
            {data && data.players.length > 0 && (
              <div className="shrink-0 flex items-center justify-between gap-3 border-t border-[#2A2116] px-3 py-2 text-[11px] text-[#8E7A5E]">
                <button
                  type="button"
                  disabled={page <= 1}
                  onClick={() => onFilters({ page: page - 1 })}
                  className={pagerClass}
                >
                  Назад
                </button>
                <span>
                  Стр. {page} из {Math.max(1, Math.ceil(data.total / limit))} · {data.total} игроков
                </span>
                <button
                  type="button"
                  disabled={page * limit >= data.total}
                  onClick={() => onFilters({ page: page + 1 })}
                  className={pagerClass}
                >
                  Далее
                </button>
              </div>
            )}
          </section>

          <SelfCard uuid={uuid} self={self} />
        </div>
      </div>
    </div>
  )
}

const pagerClass =
  'h-8 px-3 text-[11px] border border-[#2A2116] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] disabled:opacity-40 disabled:pointer-events-none transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524]'

function SelfCard({ uuid, self }: { uuid: string; self: StatsResult<PlayerStats> | null }) {
  const player = self?.data
  const totals = player?.totals
  return (
    <aside aria-label="Вы" className="bg-[#0B0906] min-h-0 overflow-hidden p-4 flex flex-col gap-2">
      <SectionHeader label="Вы" code={totals?.rank ? `#${totals.rank}` : ''} />
      {!self ? (
        <Loading />
      ) : !player && !self.error ? (
        <Empty>Вы ещё не играли</Empty>
      ) : !player ? (
        <ErrorDetail message="Ваша статистика недоступна." raw={self.error} />
      ) : (
        <>
          {self.stale && self.savedAt && (
            <p role="status" className="text-[11px] text-[#F5A524]">
              Статистика недоступна · данные на {formatTime(self.savedAt)}
            </p>
          )}
          <div className="flex items-center gap-2 text-[13px] text-neutral-100">
            <PlayerHead username={player.profile.name} />
            <span className="truncate">{player.profile.name}</span>
          </div>
          <dl className="grid grid-cols-2 gap-px bg-[#18130D] border border-[#2A2116] text-[12px]">
            <Stat label="Место">{totals?.rank ?? '—'}</Stat>
            <Stat label="MMR">{formatMmrRange(player.mmr ?? totals?.mmr)}</Stat>
            <Stat label="Бои / победы">{totals ? formatBattles(totals) : '—'}</Stat>
            <Stat label="K/D">{totals ? formatKd(totals) : '—'}</Stat>
            <Stat label="Захваты">{totals?.captures ?? '—'}</Stat>
            <Stat label="Кампании / победы">
              {player.campaigns.completed} / {player.campaigns.wins ?? '—'}
            </Stat>
          </dl>

          {player.ratingHistory.length > 0 && (
            <div>
              <h2 className={subheadClass}>Рейтинг</h2>
              <ul className="text-[11px] tabular-nums">
                {[...player.ratingHistory]
                  .sort((a, b) => b.ratedAt - a.ratedAt)
                  .slice(0, 5)
                  .map((entry) => {
                    const delta = Math.round(entry.after - entry.before)
                    return (
                      <li
                        key={entry.campaignSessionId}
                        className="flex justify-between gap-2 py-0.5"
                      >
                        <span className="text-[#8E7A5E]">{formatTime(entry.ratedAt)}</span>
                        <span className="text-[#C7AE86]">
                          {Math.round(entry.after)}{' '}
                          <span className={delta >= 0 ? 'text-[#82D66B]' : 'text-[#c98b8b]'}>
                            {delta >= 0 ? '+' : ''}
                            {delta}
                          </span>
                        </span>
                      </li>
                    )
                  })}
              </ul>
            </div>
          )}

          {player.history.length > 0 && (
            <div>
              <h2 className={subheadClass}>Последние бои</h2>
              <ul className="text-[11px] tabular-nums">
                {/* Fixed count (not scrolled): fits the panel at the min 1280x720 window. */}
                {[...player.history]
                  .sort((a, b) => b.endedAt - a.endedAt)
                  .slice(0, RECENT_MATCHES_SHOWN)
                  .map((entry) => (
                    <li key={entry.eventId} className="grid grid-cols-[1fr_auto_auto] gap-2 py-0.5">
                      <span className="truncate text-[#8E7A5E]">
                        {formatTime(entry.endedAt)} · {MODE_LABELS[entry.mode] ?? entry.mode}
                      </span>
                      <span className={resultClass(entry, uuid)}>{resultLabel(entry, uuid)}</span>
                      <span className="text-[#C7AE86]">{formatKd(entry)}</span>
                    </li>
                  ))}
              </ul>
            </div>
          )}
        </>
      )}
      <a
        href="#"
        onClick={(event) => {
          event.preventDefault()
          void openExternalUrl(`${apiBaseUrl()}/players/${uuid.toLowerCase()}`)
        }}
        className="mt-auto flex items-center gap-1.5 text-[11px] text-[#C7AE86] hover:text-[#F5A524] transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524]"
      >
        <ExternalLink size={11} /> Открыть на сайте
      </a>
    </aside>
  )
}

const subheadClass = 'mb-1 text-[10px] tracking-[0.18em] text-[#8E7A5E]'

type Outcome = Pick<
  PlayerStats['history'][number],
  'reason' | 'winner' | 'team' | 'mode' | 'winnerUuid'
>

// Mirrors blockfield-site's reasons map + outcomeLabel (src/lib/stats-outcome.ts): a match
// that didn't finish normally has no winner to report, and deathmatch tracks a winning
// player rather than a team.
const INTERRUPTED_REASONS = new Set(['abandoned', 'stopped', 'shutdown', 'error'])

function resultLabel({ reason, winner, team, mode, winnerUuid }: Outcome, ownUuid: string) {
  if (reason === 'draw') return 'Ничья'
  if (INTERRUPTED_REASONS.has(reason)) return 'Прерван'
  if (reason !== 'completed') return '—'
  if (mode === 'deathmatch') {
    if (!winnerUuid) return '—'
    return winnerUuid.toLowerCase() === ownUuid.toLowerCase() ? 'Победа' : 'Поражение'
  }
  if (winner === 'NONE' || team === 'NONE' || !winner) return 'Ничья'
  return winner === team ? 'Победа' : 'Поражение'
}

function resultClass(entry: Outcome, ownUuid: string) {
  switch (resultLabel(entry, ownUuid)) {
    case 'Победа':
      return 'text-[#82D66B]'
    case 'Поражение':
      return 'text-[#c98b8b]'
    default:
      return 'text-[#8E7A5E]'
  }
}

function FilterGroup({
  label,
  options,
  value,
  onChange,
}: {
  label: string
  options: ReadonlyArray<readonly [string, string]>
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-3">
      {options.map(([id, text]) => (
        <button
          key={id}
          type="button"
          aria-pressed={value === id}
          onClick={() => onChange(id)}
          className={`h-8 border-b text-[12px] transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524] ${value === id ? 'border-[#F5A524] text-[#F3E7D0]' : 'border-transparent text-[#8E7A5E] hover:text-[#F3E7D0]'}`}
        >
          {text}
        </button>
      ))}
    </div>
  )
}

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="bg-[#11100D] px-3 py-2">
      <dt className="text-[9px] tracking-[0.16em] text-[#8E7A5E]">{label}</dt>
      <dd className="mt-1 text-[#F3E7D0] tabular-nums">{children}</dd>
    </div>
  )
}

function Loading() {
  return (
    <div
      role="status"
      aria-label="Загрузка статистики"
      className="h-full min-h-24 grid place-items-center"
    >
      <LoaderCircle size={20} className="animate-spin text-[#8E7A5E]" />
    </div>
  )
}

function Empty({ children }: { children: ReactNode }) {
  return <p className="p-4 text-center text-[12px] text-[#8E7A5E]">{children}</p>
}
