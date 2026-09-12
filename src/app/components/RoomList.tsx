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
      <div className="max-h-56 overflow-y-auto">
        {rooms?.map((room) => (
          <article
            key={room.id}
            className="flex flex-wrap items-center justify-between gap-3 border-b border-[#2A2116] py-3"
          >
            <div className="min-w-0 flex-1">
              <h3 className="text-[14px] text-[#F3E7D0]">{room.name}</h3>
              <p className="mt-1 text-[12px] text-[#C7AE86] break-words">
                {[room.mode, room.map, phases[room.phase], `Игроков: ${room.players}`]
                  .filter(Boolean)
                  .join(' · ')}
              </p>
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
