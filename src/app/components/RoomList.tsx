import { useEffect, useId, useRef, useState } from 'react'
import { Check, ChevronDown, ChevronLeft, ChevronRight } from 'lucide-react'
import type { GameRoom } from '../../lib/api'

const phases: Record<string, string> = {
  IDLE: 'Ожидание игроков',
  VOTING: 'Голосование',
  PREPARING: 'Подготовка',
  GAME: 'Бой',
  MATCH_END: 'Матч завершён',
}

const controlClass =
  'flex h-8 shrink-0 items-center justify-center gap-2 border border-[#2A2116] px-3 text-[12px] text-[#C7AE86] transition-colors hover:border-[#8A571C] hover:text-[#F3E7D0] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#F5A524] disabled:opacity-50 disabled:cursor-not-allowed'

export function RoomList({
  rooms,
  onJoin,
  disabled,
}: {
  rooms?: GameRoom[] | null
  onJoin: (id: string) => void
  disabled: boolean
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const selectedIndex = Math.max(0, rooms?.findIndex((room) => room.id === selectedId) ?? 0)
  const room = rooms?.[selectedIndex]
  const previousRoom = rooms?.[selectedIndex - 1]
  const nextRoom = rooms?.[selectedIndex + 1]

  return (
    <section aria-label="Игровые комнаты" className="flex min-h-0 flex-1 flex-col gap-2">
      {room ? (
        <>
          <div className="flex min-w-0 items-center gap-3">
            <RoomSelect rooms={rooms ?? []} selected={room} onChange={setSelectedId} />
            <button
              type="button"
              onClick={() => onJoin(room.id)}
              disabled={disabled || !room.joinable}
              aria-label={`Войти в комнату: ${room.name}`}
              className={`${controlClass} border-[#F5A524]/50 text-[#F3E7D0]`}
            >
              Войти
            </button>
          </div>
          <article className="min-h-0 flex-1" aria-label={room.name}>
            <p
              className="truncate text-[12px] text-[#C7AE86]"
              title={[room.mode, room.map, phases[room.phase] ?? room.phase]
                .filter(Boolean)
                .join(' · ')}
            >
              {[room.mode, room.map, phases[room.phase] ?? room.phase].filter(Boolean).join(' · ')}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1 text-[12px] tabular-nums">
              <span className="text-[#C7AE86]">Игроков: {room.players}</span>
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
          {!rooms
            ? 'Список комнат недоступен. Повторим запрос автоматически.'
            : 'Открытых комнат пока нет. Создать комнату можно в игре.'}
        </p>
      )}
      <div className="mt-auto flex shrink-0 items-center justify-between gap-3 border-t border-[#2A2116] pt-2">
        <div className="flex items-center gap-2">
          {room && (
            <>
              <button
                type="button"
                aria-label="Предыдущая комната"
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
                aria-label="Следующая комната"
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
  const id = useId()
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const search = useRef({ text: '', time: 0 })
  const [open, setOpen] = useState(false)
  const [activeId, setActiveId] = useState(selected.id)
  const activeIndex = Math.max(
    0,
    rooms.findIndex((room) => room.id === activeId),
  )

  useEffect(() => {
    if (!open) return
    const closeOutside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOutside)
    return () => document.removeEventListener('pointerdown', closeOutside)
  }, [open])

  useEffect(() => {
    if (open) document.getElementById(`${id}-${activeIndex}`)?.scrollIntoView({ block: 'nearest' })
  }, [open, activeIndex, id])

  const choose = (room: GameRoom) => {
    onChange(room.id)
    setOpen(false)
    trigger.current?.focus()
  }

  return (
    <div
      ref={root}
      className="relative min-w-0 flex-1"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false)
      }}
    >
      <button
        ref={trigger}
        type="button"
        role="combobox"
        aria-label="Выбрать игровую комнату"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={id}
        aria-activedescendant={open ? `${id}-${activeIndex}` : undefined}
        onClick={() => {
          setActiveId(selected.id)
          setOpen(!open)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Tab') {
            setOpen(false)
            return
          }
          if (event.key === 'Escape') {
            event.preventDefault()
            setOpen(false)
            return
          }
          if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
            event.preventDefault()
            const index = open
              ? activeIndex
              : Math.max(
                  0,
                  rooms.findIndex((room) => room.id === selected.id),
                )
            const next =
              event.key === 'Home'
                ? 0
                : event.key === 'End'
                  ? rooms.length - 1
                  : Math.max(
                      0,
                      Math.min(rooms.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)),
                    )
            setActiveId((rooms[next] ?? selected).id)
            setOpen(true)
          } else if ((event.key === 'Enter' || event.key === ' ') && open) {
            event.preventDefault()
            choose(rooms[activeIndex] ?? selected)
          } else if (
            event.key.length === 1 &&
            event.key !== ' ' &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey
          ) {
            event.preventDefault()
            const now = Date.now()
            search.current.text =
              (now - search.current.time < 700 ? search.current.text : '') +
              event.key.toLocaleLowerCase()
            search.current.time = now
            const match = rooms.find((room) =>
              room.name.toLocaleLowerCase().startsWith(search.current.text),
            )
            if (match) {
              setActiveId(match.id)
              setOpen(true)
            }
          }
        }}
        className={`flex h-8 w-full min-w-0 items-center justify-between gap-3 border bg-[#11100D] px-3 text-left text-[14px] text-[#F3E7D0] transition-colors hover:border-[#8A571C] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#F5A524] ${open ? 'border-[#8A571C]' : 'border-[#2A2116]'}`}
      >
        <span className="truncate">{selected.name}</span>
        <ChevronDown
          aria-hidden="true"
          size={14}
          className={`shrink-0 text-[#C7AE86] ${open ? 'rotate-180' : ''}`}
        />
      </button>
      <ul
        id={id}
        role="listbox"
        aria-label="Игровые комнаты"
        hidden={!open}
        className="absolute inset-x-0 top-full z-50 mt-1 max-h-48 overflow-y-auto overscroll-contain border border-[#8A571C] bg-[#11100D] p-1 [scrollbar-color:#8A571C_#11100D] [scrollbar-width:thin]"
      >
        {rooms.map((room, index) => (
          <li
            key={room.id}
            id={`${id}-${index}`}
            role="option"
            aria-selected={room.id === selected.id}
            onPointerMove={() => setActiveId(room.id)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(room)}
            className={`flex min-h-9 cursor-pointer items-center gap-3 px-2 py-2 text-[14px] ${index === activeIndex ? 'bg-[#2A2116] text-[#F3E7D0]' : 'text-[#C7AE86]'} ${room.id === selected.id ? 'text-[#F5A524]' : ''}`}
          >
            <span className="min-w-0 flex-1 break-words">{room.name}</span>
            {room.id === selected.id && (
              <Check aria-hidden="true" size={14} className="shrink-0 text-[#F5A524]" />
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
