import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { RoomList } from './RoomList'

describe('RoomList', () => {
  it('keeps the lobby available when no rooms are open', () => {
    const html = renderToStaticMarkup(<RoomList rooms={[]} onJoin={() => {}} disabled={false} />)
    expect(html).toContain('Сейчас матчей нет')
    expect(html).toContain('Войти в лобби')
  })
  it('keeps the lobby open while the game server updates', () => {
    const html = renderToStaticMarkup(
      <RoomList rooms={[]} updating onJoin={() => {}} disabled={false} />,
    )
    expect(html).toContain('Игровой сервер обновляется — лобби открыто')
    expect(html).not.toContain('Сейчас матчей нет')
    expect(html).toContain('Войти в лобби')
  })
  it('shows the maintenance message instead of the generic update text', () => {
    const html = renderToStaticMarkup(
      <RoomList
        rooms={[]}
        updating
        maintenance="Меняем карту, 10 минут"
        onJoin={() => {}}
        disabled={false}
      />,
    )
    expect(html).toContain('Идут технические работы: Меняем карту, 10 минут')
    expect(html).not.toContain('Игровой сервер обновляется')
    expect(html).toContain('Войти в лобби')
    expect(
      renderToStaticMarkup(
        <RoomList rooms={[]} updating maintenance="" onJoin={() => {}} disabled={false} />,
      ),
    ).toContain('Идут технические работы.')
  })
  it('shows one room at a time while keeping every room selectable', () => {
    const rooms = Array.from({ length: 20 }, (_, index) => ({
      id: `room-${index}`,
      name: `Матч ${index + 1}`,
      mode: 'Захват точек',
      map: 'Город',
      phase: 'IDLE',
      players: index,
      joinable: true,
    }))
    const html = renderToStaticMarkup(<RoomList rooms={rooms} onJoin={() => {}} disabled={false} />)
    expect(html.match(/<article/g)).toHaveLength(1)
    expect(html.match(/role="option"/g)).toHaveLength(20)
    expect(html).toContain('Войти: Матч 1')
    expect(html).not.toContain('Войти: Матч 20')
  })

  it('explains an unavailable list and keeps the lobby action', () => {
    const html = renderToStaticMarkup(<RoomList rooms={null} onJoin={() => {}} disabled={false} />)
    expect(html).toContain('Список матчей недоступен')
    expect(html).toContain('Войти в лобби')
  })

  it('shows live scores and point control with an accessible progress value', () => {
    const html = renderToStaticMarkup(
      <RoomList
        rooms={[
          {
            id: '12345678-1234-1234-1234-123456789abc',
            name: 'Матч 1',
            mode: 'Захват точек',
            map: 'Город',
            phase: 'GAME',
            players: 8,
            joinable: true,
            battle: {
              red: 720,
              blue: 685,
              points: [
                {
                  id: 'a',
                  name: 'A',
                  owner: 'BLUE',
                  claiming: 'NONE',
                  capturing: 'RED',
                  progress: 35,
                },
              ],
            },
          },
        ]}
        onJoin={() => {}}
        disabled={false}
      />,
    )
    expect(html).toContain('Красные: 720')
    expect(html).toContain('Синие: 685')
    expect(html).toContain('Красные нейтрализуют')
    expect(html).toContain('aria-valuenow="35"')
  })

  it('shows the Ranked format, ready count and admission state', () => {
    const room = {
      id: '12345678-1234-1234-1234-123456789abc',
      name: 'Матч 1',
      mode: 'Захват точек',
      map: 'Город',
      phase: 'PREPARING',
      players: 4,
      joinable: true,
      format: 'ranked' as const,
      admission: 'waiting' as const,
      ready: { ready: 3, required: 4 },
    }
    const html = renderToStaticMarkup(
      <RoomList rooms={[room]} onJoin={() => {}} disabled={false} />,
    )
    expect(html).toContain('Ranked · Захват точек')
    expect(html).toContain('Готовы: 3/4')
    expect(html).toContain('Встать в очередь')
  })

  it('disables joining a closed room even when the server reports it joinable', () => {
    const room = {
      id: '12345678-1234-1234-1234-123456789abc',
      name: 'Матч 1',
      mode: 'Захват точек',
      map: 'Город',
      phase: 'GAME',
      players: 4,
      joinable: true,
      admission: 'closed' as const,
    }
    const html = renderToStaticMarkup(
      <RoomList rooms={[room]} onJoin={() => {}} disabled={false} />,
    )
    expect(html).toContain('Мест нет')
    expect(html).toMatch(/disabled=""[^>]*aria-label="Мест нет: Матч 1"/)
  })
})
