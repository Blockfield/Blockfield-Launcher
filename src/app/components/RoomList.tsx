import type { GameRoom } from '../../lib/api'

const phases: Record<string, string> = {
  IDLE: 'Ожидание игроков',
  VOTING: 'Голосование',
  PREPARING: 'Подготовка',
  GAME: 'Бой',
  MATCH_END: 'Матч завершён',
}
export function RoomList({
  rooms,
  onJoin,
  disabled,
}: {
  rooms?: GameRoom[] | null
  onJoin: (id: string) => void
  disabled: boolean
}) {
  if (rooms?.length === 0) return null
  return (
    <section aria-labelledby="rooms-title" className="mt-4 border-t border-[#2A2116] pt-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="rooms-title" className="text-[16px] text-[#F3E7D0]">
          Игровые комнаты
        </h2>
        <button
          type="button"
          onClick={() => onJoin('lobby')}
          disabled={disabled}
          className="border border-[#F5A524] px-4 py-3 text-[12px] text-[#F3E7D0] hover:bg-[#2A2116] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-[#F5A524] disabled:opacity-50"
        >
          Войти в лобби
        </button>
      </div>
      <p role="status" className="mt-2 text-[12px] text-[#C7AE86]">
        {!rooms
          ? 'Список комнат недоступен. Повторим запрос автоматически.'
          : rooms.length === 0
            ? 'Открытых комнат пока нет. Создать комнату можно в игре.'
            : 'Состояние обновляется каждые 10 секунд.'}
      </p>
      <div className="max-h-96 overflow-y-auto">
        {rooms?.map((room) => (
          <article
            key={room.id}
            className="flex flex-col items-stretch justify-between gap-3 border-b sm:flex-row sm:items-center border-[#2A2116] py-3"
          >
            <div className="min-w-0 flex-1">
              <h3 className="text-[14px] text-[#F3E7D0]">{room.name}</h3>
              <p className="mt-1 text-[12px] text-[#C7AE86] break-words">
                {[room.mode, room.map, phases[room.phase], `Игроков: ${room.players}`]
                  .filter(Boolean)
                  .join(' · ')}
              </p>
              {room.battle && (
                <div className="mt-3">
                  <div
                    aria-label="Очки команд"
                    className="flex flex-wrap gap-5 text-[14px] font-semibold tabular-nums"
                  >
                    <span className="text-[#FF9D99]">Красные: {room.battle.red}</span>
                    <span className="text-[#91C8FF]">Синие: {room.battle.blue}</span>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-x-5 gap-y-3">
                    {room.battle.points.map((point) => {
                      const team = point.owner === 'NEUTRAL' ? point.claiming : point.owner
                      const owner =
                        team === 'RED' ? 'Красные' : team === 'BLUE' ? 'Синие' : 'Нейтральная'
                      const action =
                        point.capturing !== 'NONE' && point.capturing !== team
                          ? ` · ${point.capturing === 'RED' ? 'Красные' : 'Синие'} нейтрализуют`
                          : ''
                      const label = `${point.name} · ${owner} · ${point.progress}%${action}`
                      return (
                        <div
                          key={point.id}
                          className="flex min-w-36 flex-1 flex-col justify-between text-[12px] text-[#C7AE86]"
                        >
                          <p>{label}</p>
                          <div
                            role="progressbar"
                            aria-label={label}
                            aria-valuenow={point.progress}
                            aria-valuemin={0}
                            aria-valuemax={100}
                            className="mt-2 h-1.5 bg-[#2A2116]"
                          >
                            <div
                              className="h-full"
                              style={{
                                width: `${point.progress}%`,
                                backgroundColor:
                                  team === 'RED'
                                    ? '#FF9D99'
                                    : team === 'BLUE'
                                      ? '#91C8FF'
                                      : '#C7AE86',
                              }}
                            />
                          </div>
                        </div>
                      )
                    })}
                  </div>
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={() => onJoin(room.id)}
              disabled={disabled || !room.joinable}
              aria-label={`Войти в комнату: ${room.name}`}
              className="shrink-0 border border-[#F5A524] px-4 py-3 text-[12px] text-[#F3E7D0] hover:bg-[#2A2116] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-[#F5A524] disabled:opacity-50"
            >
              Войти в комнату
            </button>
          </article>
        ))}
      </div>
    </section>
  )
}
