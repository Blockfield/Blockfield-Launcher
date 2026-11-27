import { beforeEach, expect, it, vi } from 'vitest'
import type { GameStatus } from './game-state'
const { invoke, listen, stop, emit } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  stop: vi.fn(),
  emit: {
    current: (event: { payload: GameStatus }) => {
      void event
    },
  },
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen }))
beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  invoke.mockResolvedValue({ phase: 'idle', revision: 0 })
  listen.mockImplementation(async (_event, handler) => {
    emit.current = handler
    return stop
  })
})

it('does not overwrite a running event with a stale startup snapshot', async () => {
  const api = await import('./game-state')
  let resolve!: (status: GameStatus) => void
  invoke.mockImplementationOnce(
    () =>
      new Promise<GameStatus>((done) => {
        resolve = done
      }),
  )
  const pending = api.watchGameState()
  await vi.waitFor(() => expect(invoke).toHaveBeenCalled())
  emit.current({ payload: { phase: 'running', revision: 2 } })
  resolve({ phase: 'idle', revision: 0 })
  const unlisten = await pending
  expect(api.getGameState().phase).toBe('running')
  emit.current({ payload: { phase: 'finishing', revision: 3 } })
  expect(api.getGameState().phase).toBe('finishing')
  emit.current({ payload: { phase: 'idle', revision: 4 } })
  emit.current({ payload: { phase: 'running', revision: 2 } })
  expect(api.getGameState().phase).toBe('idle')
  unlisten()
  emit.current({ payload: { phase: 'running', revision: 5 } })
  expect(api.getGameState().phase).toBe('idle')
})

it('shares simultaneous launches and rejects further attempts while the game is running', async () => {
  const api = await import('./game-state')
  await api.watchGameState()
  let finish!: () => void
  invoke.mockImplementation((command: string) =>
    command === 'launch_game'
      ? new Promise<void>((resolve) => {
          finish = resolve
        })
      : Promise.resolve({ phase: 'running', revision: 2 }),
  )
  const pending = api.launchGame()
  expect(api.launchGame()).toBe(pending)
  finish()
  await pending
  await expect(api.launchGame()).rejects.toThrow('Игра уже запущена')
  expect(invoke.mock.calls.filter(([cmd]) => cmd === 'launch_game')).toHaveLength(1)
})

it('does not invent a running state if the game exits before launch IPC completes', async () => {
  const api = await import('./game-state')
  await api.watchGameState()
  invoke.mockImplementation(async (command: string) =>
    command === 'game_status' ? { phase: 'idle', revision: 4 } : undefined,
  )
  await api.launchGame()
  expect(api.getGameState()).toEqual({ phase: 'idle', revision: 4 })
})

it('releases the launch request after a failed pre-launch command', async () => {
  const api = await import('./game-state')
  await api.watchGameState()
  invoke.mockRejectedValueOnce(new Error('Command failed'))
  await expect(api.launchGame()).rejects.toThrow('Command failed')
  expect(api.getGameState().phase).toBe('idle')
  await api.launchGame()
  expect(invoke.mock.calls.filter(([cmd]) => cmd === 'launch_game')).toHaveLength(2)
})
