import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { RoomList } from './RoomList'

describe('RoomList', () => {
  it('hides a confirmed empty room list', () => {
    expect(renderToStaticMarkup(<RoomList rooms={[]} onJoin={() => {}} disabled={false} />)).toBe(
      '',
    )
  })
  it('shows live scores and point control with an accessible progress value', () => {
    const html = renderToStaticMarkup(
      <RoomList
        rooms={[
          {
            id: '12345678-1234-1234-1234-123456789abc',
            name: 'Комната 1',
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
})
