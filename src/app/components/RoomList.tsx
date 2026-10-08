import { useEffect, useId, useRef, useState } from 'react'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { Select } from './Select'
import type { GameRoom } from '../../lib/api'

const phases: Record<string, string> = {
  IDLE: 'Ожидание игроков',
  VOTING: 'Голосование',
  PREPARING: 'Подготовка',
  GAME: 'Бой',
  MATCH_END: 'Матч завершён',
}

const formats: Record<string, string> = { casual: 'Casual', ranked: 'Ranked' }
// Mirrors CampaignMode.joinHint in the mod: open — join now, waiting — queue only, closed — full or running.
const joinLabels: Record<string, string> = {
  open: 'Войти',
  waiting: 'Встать в очередь',
  closed: 'Мест нет',
}

export const GAME_UPDATING = 'Игровой сервер обновляется — лобби открыто'
export const GAME_MAINTENANCE = 'Идут технические работы'

const controlClass =
  'flex h-8 shrink-0 items-center justify-center gap-2 border border-[#2A2116] px-3 text-[12px] text-[#C7AE86] transition-colors hover:border-[#8A571C] hover:text-[#F3E7D0] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#F5A524] disabled:opacity-50 disabled:cursor-not-allowed'

export function RoomList({
  rooms,
  onJoin,
  disabled,
  updating = false,
  maintenance,
}: {
  rooms?: GameRoom[] | null
  onJoin: (id: string) => void
  disabled: boolean
  updating?: boolean
  maintenance?: string | null
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const selectedIndex = Math.max(0, rooms?.findIndex((room) => room.id === selectedId) ?? 0)
  const room = rooms?.[selectedIndex]
  const previousRoom = rooms?.[selectedIndex - 1]
  const nextRoom = rooms?.[selectedIndex + 1]

  return (
    <section aria-label="Идущие матчи" className="flex min-h-0 flex-1 flex-col gap-2">
      {room ? (
        <>
          <div className="flex min-w-0 items-center gap-3">
            <RoomSelect rooms={rooms ?? []} selected={room} onChange={setSelectedId} />
            <button
              type="button"
              onClick={() => onJoin(room.id)}
              disabled={disabled || updating || !room.joinable || room.admission === 'closed'}
              aria-label={`${joinLabels[room.admission ?? 'open']}: ${room.name}`}
              className={`${controlClass} border-[#F5A524]/50 text-[#F3E7D0]`}
            >
              {joinLabels[room.admission ?? 'open']}
            </button>
          </div>
          <article className="min-h-0 flex-1" aria-label={room.name}>
            <p
              className="truncate text-[12px] text-[#C7AE86]"
              title={[
                room.format && formats[room.format],
                room.mode,
                room.map,
                phases[room.phase] ?? room.phase,
              ]
                .filter(Boolean)
                .join(' · ')}
            >
              {[
                room.format && formats[room.format],
                room.mode,
                room.map,
                phases[room.phase] ?? room.phase,
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1 text-[12px] tabular-nums">
              <span className="text-[#C7AE86]">Игроков: {room.players}</span>
              {room.ready && (
                <span className="text-[#C7AE86]">
                  Готовы: {room.ready.ready}/{room.ready.required}
                </span>
              )}
              {room.battle && (
                <div aria-label="Очки команд" className="flex gap-4 font-semibold">
                  <span className="text-[#FF9D99]">Красные: {room.battle.red}</span>
                  <span className="text-[#91C8FF]">Синие: {room.battle.blue}</span>
                </div>
              )}
            </div>
            {room.battle && <BattlePoints key={room.id} points={room.battle.points} />}
          </article>
        </>
      ) : (
        <p role="status" className="flex-1 text-[12px] leading-relaxed text-[#C7AE86]">
          {maintenance != null
            ? `${GAME_MAINTENANCE}${maintenance ? `: ${maintenance}` : '.'} Матчи откроются после завершения работ.`
            : updating
              ? `${GAME_UPDATING}. Нажмите «Играть» — вас перенесут в игру, когда сервер запустится.`
              : !rooms
                ? 'Список матчей недоступен. Повторим запрос автоматически.'
                : 'Сейчас матчей нет. Нажмите «Играть» и выберите режим в хабе: Casual или Рейтинг.'}
        </p>
      )}
      <div className="mt-auto flex shrink-0 items-center justify-between gap-3 border-t border-[#2A2116] pt-2">
        <div className="flex items-center gap-2">
          {room && (
            <>
              <button
                type="button"
                aria-label="Предыдущий матч"
                disabled={!previousRoom}
                onClick={() => previousRoom && setSelectedId(previousRoom.id)}
                className={controlClass}
              >
                <ChevronLeft size={14} />
              </button>
              <span role="status" className="text-[12px] tabular-nums text-[#C7AE86]">
                {selectedIndex + 1} / {rooms?.length}
              </span>
              <button
                type="button"
                aria-label="Следующий матч"
                disabled={!nextRoom}
                onClick={() => nextRoom && setSelectedId(nextRoom.id)}
                className={controlClass}
              >
                <ChevronRight size={14} />
              </button>
            </>
          )}
        </div>
        <button
          type="button"
          onClick={() => onJoin('lobby')}
          disabled={disabled}
          className={controlClass}
        >
          Войти в лобби
        </button>
      </div>
    </section>
  )
}

function BattlePoints({ points }: { points: NonNullable<GameRoom['battle']>['points'] }) {
  const [page, setPage] = useState(0)
  const pages = Math.ceil(points.length / 3)
  const currentPage = Math.min(page, Math.max(0, pages - 1))

  return (
    <div className="mt-2 flex items-center gap-2">
      <div className="grid min-w-0 flex-1 grid-cols-3 gap-3">
        {points.slice(currentPage * 3, currentPage * 3 + 3).map((point) => {
          const team = point.owner === 'NEUTRAL' ? point.claiming : point.owner
          const owner = team === 'RED' ? 'Красные' : team === 'BLUE' ? 'Синие' : 'Нейтральная'
          const action =
            point.capturing !== 'NONE' && point.capturing !== team
              ? ` · ${point.capturing === 'RED' ? 'Красные' : 'Синие'} нейтрализуют`
              : ''
          const label = `${point.name} · ${owner} · ${point.progress}%${action}`
          return (
            <div key={point.id} className="min-w-0 text-[11px] text-[#C7AE86]" title={label}>
              <p className="truncate">
                {point.name} · {owner}
              </p>
              <div
                role="progressbar"
                aria-label={label}
                aria-valuenow={point.progress}
                aria-valuemin={0}
                aria-valuemax={100}
                className="mt-1 h-1.5 bg-[#2A2116]"
              >
                <div
                  className="h-full"
                  style={{
                    width: `${point.progress}%`,
                    backgroundColor:
                      team === 'RED' ? '#FF9D99' : team === 'BLUE' ? '#91C8FF' : '#C7AE86',
                  }}
                />
              </div>
              <p className="mt-1 truncate tabular-nums">
                {point.progress}%{action}
              </p>
            </div>
          )
        })}
      </div>
      {pages > 1 && (
        <button
          type="button"
          aria-label={`Следующие точки захвата, страница ${currentPage + 1} из ${pages}`}
          onClick={() => setPage((currentPage + 1) % pages)}
          className={controlClass}
        >
          {currentPage + 1}/{pages}
          <ChevronRight size={14} />
        </button>
      )}
    </div>
  )
}

function RoomSelect({
  rooms,
  selected,
  onChange,
}: {
  rooms: GameRoom[]
  selected: GameRoom
  onChange: (id: string) => void
}) {
  return (
    <Select
      ariaLabel="Выбрать матч"
      listboxLabel="Идущие матчи"
      value={selected.id}
      options={rooms.map((room) => ({ value: room.id, label: room.name }))}
      className="min-w-0 flex-1"
      onChange={onChange}
    />
  )
}
