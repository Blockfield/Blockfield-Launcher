// @vitest-environment jsdom
import { act, StrictMode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { RoomList } from './RoomList'
import { LauncherUpdatePanel } from './LauncherUpdatePanel'
import { Shell } from './Shell'
import { MainScreen, ProfileSelect } from './MainScreen'
import { SettingsScreen } from './SettingsScreen'
import App from '../App'
import { checkLauncherUpdate, installLauncherUpdate } from '../../lib/launcher-update'

const { invoke, getVersion, relaunch, listen, channels } = vi.hoisted(() => ({
  invoke: vi.fn(),
  getVersion: vi.fn(),
  relaunch: vi.fn(),
  listen: vi.fn(async (_event?: any, _handler?: any) => () => {}),
  channels: [] as Array<{
    onmessage: (event: { phase: string; downloaded: number; total: number | null }) => void
  }>,
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage = () => {}
    constructor() {
      channels.push(this)
    }
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch }))

const rooms = [
  {
    id: 'first',
    name: 'Матч 1',
    mode: 'Захват точек',
    map: 'Город',
    phase: 'IDLE',
    players: 2,
    joinable: true,
  },
  {
    id: 'second',
    name: 'Матч 2',
    mode: 'Захват точек',
    map: 'Город',
    phase: 'VOTING',
    players: 4,
    joinable: true,
  },
]
const scrollIntoView = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView')
let container: HTMLDivElement
let root: Root | null

beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
    configurable: true,
    value: vi.fn(),
  })
  container = document.createElement('div')
  document.body.append(container)
  root = createRoot(container)
})

afterEach(async () => {
  await act(async () => root?.unmount())
  container.remove()
  if (scrollIntoView) {
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', scrollIntoView)
  } else {
    Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
  }
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

it('selects and joins a room by keyboard, retaining focus and decorative Lucide icons', async () => {
  const onJoin = vi.fn()
  const errors = vi.spyOn(console, 'error')
  const addListener = vi.spyOn(document, 'addEventListener')
  const removeListener = vi.spyOn(document, 'removeEventListener')
  await act(async () => {
    root!.render(
      <StrictMode>
        <RoomList rooms={rooms} onJoin={onJoin} disabled={false} />
      </StrictMode>,
    )
  })
  const trigger = container.querySelector<HTMLButtonElement>('[role="combobox"]')!
  trigger.focus()
  await act(async () => {
    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
  })
  const active = document.getElementById(trigger.getAttribute('aria-activedescendant')!)
  expect(trigger.getAttribute('aria-expanded')).toBe('true')
  expect(active?.textContent).toContain('Матч 2')
  expect(trigger.querySelector('svg[aria-hidden="true"] path')).not.toBeNull()
  await act(async () => {
    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  })
  expect(trigger.getAttribute('aria-expanded')).toBe('false')
  expect(document.activeElement).toBe(trigger)
  expect(container.querySelector('article')?.getAttribute('aria-label')).toBe('Матч 2')
  await act(async () => {
    container.querySelector<HTMLButtonElement>('[aria-label="Войти: Матч 2"]')!.click()
  })
  expect(onJoin).toHaveBeenCalledExactlyOnceWith('second')
  await act(async () => trigger.click())
  const outsideListener = addListener.mock.calls
    .filter(([type]) => type === 'pointerdown')
    .pop()![1]
  await act(async () => root!.unmount())
  root = null
  expect(removeListener).toHaveBeenCalledWith('pointerdown', outsideListener)
  expect(errors).not.toHaveBeenCalled()
})

it('disables joining a match during an update while preserving the lobby action', async () => {
  const onJoin = vi.fn()
  await act(async () => {
    root!.render(<RoomList rooms={rooms} updating onJoin={onJoin} disabled={false} />)
  })
  const join = container.querySelector<HTMLButtonElement>('[aria-label="Войти: Матч 1"]')!
  await act(async () => join.click())
  expect(join.disabled).toBe(true)
  expect(onJoin).not.toHaveBeenCalled()
  const lobby = Array.from(container.querySelectorAll('button')).find(
    (button) => button.textContent === 'Войти в лобби',
  )!
  await act(async () => lobby.click())
  expect(onJoin).toHaveBeenCalledExactlyOnceWith('lobby')
})

it('shows the selected Workshop target or its unset state and restores the Game address', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify({ serverIp: 'game.example:25565' }))),
  )
  const render = async (serverAddress?: string) => {
    await act(async () =>
      root!.render(
        <Shell
          active="main"
          user={{ username: 'ProfileTest', role: 'игрок' }}
          serverAddress={serverAddress}
          onNavigate={() => {}}
          onLauncherUpdate={() => {}}
        >
          <div />
        </Shell>,
      ),
    )
  }
  await render()
  await vi.waitFor(() =>
    expect(container.querySelector('footer')?.textContent).toContain('game.example:25565'),
  )
  await render('raknet;workshop.example:25566')
  expect(container.querySelector('footer')?.textContent).toContain('raknet;workshop.example:25566')
  expect(container.querySelector('footer')?.textContent).not.toContain('game.example:25565')
  await render('')
  expect(container.querySelector('footer')?.textContent).toContain('Адрес мастерской не задан')
  expect(container.querySelector('footer')?.textContent).not.toContain('game.example:25565')
  await render()
  expect(container.querySelector('footer')?.textContent).toContain('game.example:25565')
})

it('renders update channel progress, rejects a failed signature, and waits for an explicit restart', async () => {
  getVersion.mockResolvedValue('1.0.6')
  invoke.mockResolvedValue({ currentVersion: '1.0.6', version: '1.0.7', body: 'Fixes' })
  await act(async () => root!.render(<LauncherUpdatePanel />))
  await act(async () => {
    container.querySelector<HTMLButtonElement>('button')!.click()
    await checkLauncherUpdate()
  })
  expect(invoke).toHaveBeenCalledExactlyOnceWith('check_launcher_update')
  expect(container.querySelector('[role="status"]')?.textContent).toBe('Доступна версия 1.0.7')
  expect(container.querySelector('button svg path')).not.toBeNull()

  let reject!: (error: Error) => void
  invoke.mockImplementationOnce(
    () =>
      new Promise<void>((_, fail) => {
        reject = fail
      }),
  )
  await act(async () => container.querySelectorAll<HTMLButtonElement>('button')[1]!.click())
  await act(async () => {
    channels[0]!.onmessage({ phase: 'downloading', downloaded: 1048576, total: 2097152 })
  })
  expect(container.querySelector('progress')?.value).toBe(50)
  expect(Array.from(container.querySelectorAll('button')).every((button) => button.disabled)).toBe(
    true,
  )
  await act(async () => {
    channels[0]!.onmessage({ phase: 'verifying', downloaded: 0, total: null })
  })
  expect(container.querySelector('progress')?.value).toBe(100)
  expect(container.textContent).toContain('1.0 МБ / 2.0 МБ')
  await act(async () => {
    reject(new Error('Invalid signature'))
    await installLauncherUpdate()
  })
  expect(container.querySelector('[role="alert"]')?.textContent).toContain('Invalid signature')
  expect(container.textContent).not.toContain('Перезапустить лаунчер')

  invoke.mockResolvedValueOnce(undefined)
  await act(async () => {
    container.querySelectorAll<HTMLButtonElement>('button')[1]!.click()
    await installLauncherUpdate()
  })
  await act(async () => {
    channels[0]!.onmessage({ phase: 'downloading', downloaded: 1, total: 20 })
  })
  expect(container.querySelector('[role="status"]')?.textContent).toContain(
    'Версия 1.0.7 установлена',
  )
  expect(container.querySelector('button')?.textContent).toContain('Перезапустить лаунчер')
  expect(relaunch).not.toHaveBeenCalled()
})

it('navigates custom profile select via keyboard, shows checkmark, and respects disabled state', async () => {
  const onChange = vi.fn()
  const addListener = vi.spyOn(document, 'addEventListener')
  const removeListener = vi.spyOn(document, 'removeEventListener')

  await act(async () => {
    root!.render(
      <StrictMode>
        <ProfileSelect value="game" disabled={false} onChange={onChange} />
      </StrictMode>,
    )
  })

  const trigger = container.querySelector<HTMLButtonElement>('[role="combobox"]')!
  expect(trigger).not.toBeNull()
  expect(trigger.id).toBe('client-profile')
  expect(trigger.getAttribute('aria-label')).toBe('Профиль')
  expect(trigger.getAttribute('aria-expanded')).toBe('false')
  expect(trigger.textContent).toContain('Игра')

  // Open via ArrowDown
  trigger.focus()
  await act(async () => {
    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
  })
  expect(trigger.getAttribute('aria-expanded')).toBe('true')

  const listbox = container.querySelector('[role="listbox"]')!
  expect(listbox).not.toBeNull()
  const options = container.querySelectorAll('[role="option"]')
  expect(options.length).toBe(2)
  const opt0 = options[0]!
  const opt1 = options[1]!
  expect(opt0.textContent).toContain('Игра')
  expect(opt1.textContent).toContain('Мастерская')

  // First option is selected so it has the check icon
  expect(opt0.getAttribute('aria-selected')).toBe('true')
  expect(opt0.querySelector('svg')).not.toBeNull()
  expect(opt1.getAttribute('aria-selected')).toBe('false')
  expect(opt1.querySelector('svg')).toBeNull()

  // Press Enter on active item (which is now options[1] / workshop)
  await act(async () => {
    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  })
  expect(onChange).toHaveBeenCalledWith('workshop')
  expect(trigger.getAttribute('aria-expanded')).toBe('false')

  // Verify outside pointer listener clean up
  await act(async () => trigger.click())
  const outsideListener = addListener.mock.calls
    .filter(([type]) => type === 'pointerdown')
    .pop()![1]
  await act(async () => root!.unmount())
  root = null
  expect(removeListener).toHaveBeenCalledWith('pointerdown', outsideListener)
})

it('hides profile select from ordinary or unloaded users and displays it only when authorized', async () => {
  const onSelectProfile = vi.fn()

  // 1. Unauthorized / unloaded user has no onSelectProfile callback
  await act(async () => {
    root!.render(
      <MainScreen
        onPlay={() => {}}
        profile="game"
        onSelectProfile={undefined}
      />,
    )
  })
  expect(container.querySelector('#client-profile')).toBeNull()
  expect(container.textContent).not.toContain('Профиль')

  // 2. Authorized user receives onSelectProfile callback
  await act(async () => {
    root!.render(
      <MainScreen
        onPlay={() => {}}
        profile="game"
        onSelectProfile={onSelectProfile}
      />,
    )
  })
  const select = container.querySelector('#client-profile')
  expect(select).not.toBeNull()
  expect(select?.getAttribute('role')).toBe('combobox')
})

it('hides workshop settings from unauthorized accounts in SettingsScreen', async () => {
  // 1. Unauthorized account: workshop server input and workshop directory label are hidden
  await act(async () => {
    root!.render(
      <SettingsScreen
        username="OrdinaryPlayer"
        canWorkshop={false}
        profile="game"
        onAccountChange={() => {}}
        tab="general"
        onTabChange={() => {}}
      />,
    )
    await Promise.resolve()
  })
  expect(container.querySelector('input[aria-label="Адрес мастерской"]')).toBeNull()
  expect(container.textContent).not.toContain('Папка мастерской')
  expect(container.textContent).not.toContain('Адрес мастерской')

  // 2. Authorized account on workshop profile: workshop settings are visible
  await act(async () => {
    root!.render(
      <SettingsScreen
        username="AuthorizedBuilder"
        canWorkshop={true}
        profile="workshop"
        onAccountChange={() => {}}
        tab="general"
        onTabChange={() => {}}
      />,
    )
    await Promise.resolve()
  })
  expect(container.querySelector('input[aria-label="Адрес мастерской"]')).not.toBeNull()
  expect(container.textContent).toContain('Папка мастерской')
})

it('preserves owner workshop profile on boot and does not overwrite during initial load', async () => {
  ;(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {}
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'load_settings') {
      return { activeProfile: 'workshop', workshopServer: 'workshop.blockfield.pro:25565', username: 'OwnerUser' }
    }
    if (cmd === 'account_status') {
      return { loggedIn: true, uuid: 'owner-uuid-1', username: 'OwnerUser' }
    }
    if (cmd === 'stats_player') {
      return { profile: { uuid: 'owner-uuid-1', role: 'owner' } }
    }
    if (cmd === 'check_modpack_version') {
      return { localVersion: '1.0.0', remoteVersion: '1.0.0', needsUpdate: false, javaOk: true, loaderOk: true }
    }
    if (cmd === 'game_status') {
      return { phase: 'idle', revision: 0 }
    }
    return null
  })

  await act(async () => {
    root!.render(<App />)
  })

  // Give microtasks and state resolution time to settle
  await vi.waitFor(() => {
    expect(container.querySelector('#client-profile')).not.toBeNull()
  })

  // Assert select_profile was NEVER invoked (workshop preserved)
  const selectCalls = invoke.mock.calls.filter(([cmd]) => cmd === 'select_profile')
  expect(selectCalls.length).toBe(0)
  expect(container.querySelector('#client-profile')?.textContent).toContain('Мастерская')
})

it('blocks actions for unauthorized user on stored workshop profile until persisted switch to game completes', async () => {
  ;(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {}
  let resolveSelectProfile!: (cfg: unknown) => void
  const selectProfilePromise = new Promise((resolve) => {
    resolveSelectProfile = resolve
  })

  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'load_settings') {
      return { activeProfile: 'workshop', workshopServer: 'workshop.blockfield.pro:25565', username: 'PlayerUser' }
    }
    if (cmd === 'account_status') {
      return { loggedIn: true, uuid: 'player-uuid-1', username: 'PlayerUser' }
    }
    if (cmd === 'stats_player') {
      return { profile: { uuid: 'player-uuid-1', role: 'player' } }
    }
    if (cmd === 'select_profile') {
      return selectProfilePromise
    }
    if (cmd === 'check_modpack_version') {
      return { localVersion: '1.0.0', remoteVersion: '1.0.0', needsUpdate: false, javaOk: true, loaderOk: true }
    }
    if (cmd === 'game_status') {
      return { phase: 'idle', revision: 0 }
    }
    return null
  })

  await act(async () => {
    root!.render(<App />)
  })

  // Auth resolved: player has no workshop access -> selectProfile('game') triggered
  await vi.waitFor(() => {
    const selectCalls = invoke.mock.calls.filter(([cmd]) => cmd === 'select_profile')
    expect(selectCalls.length).toBe(1)
  })

  // While switch is pending: deploy button must be disabled and status indicates syncing
  const deployBtn = container.querySelector<HTMLButtonElement>('button.group')
  expect(deployBtn?.disabled).toBe(true)

  // Now resolve the select_profile switch
  await act(async () => {
    resolveSelectProfile({ activeProfile: 'game', workshopServer: '' })
  })

  // Once resolved: profile is game, selector is hidden (since player is unauthorized)
  await vi.waitFor(() => {
    expect(container.querySelector('#client-profile')).toBeNull()
  })
})

it('handles select_profile failure when game is busy by showing visible error and retrying when idle', async () => {
  ;(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {}
  let selectAttempt = 0
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'load_settings') {
      return { activeProfile: 'workshop', workshopServer: 'workshop.blockfield.pro:25565', username: 'PlayerUser' }
    }
    if (cmd === 'account_status') {
      return { loggedIn: true, uuid: 'player-uuid-1', username: 'PlayerUser' }
    }
    if (cmd === 'stats_player') {
      return { profile: { uuid: 'player-uuid-1', role: 'player' } }
    }
    if (cmd === 'select_profile') {
      selectAttempt++
      if (selectAttempt === 1) {
        throw new Error('Закройте игру перед сменой профиля.')
      }
      return { activeProfile: 'game' }
    }
    if (cmd === 'check_modpack_version') {
      return { localVersion: '1.0.0', remoteVersion: '1.0.0', needsUpdate: false, javaOk: true, loaderOk: true }
    }
    if (cmd === 'game_status') {
      return { phase: 'running', revision: 1 }
    }
    return null
  })

  await act(async () => {
    root!.render(<App />)
  })

  // First attempt fails: visible error is displayed and deploy button is disabled
  await vi.waitFor(() => {
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      'Закройте игру перед сменой профиля',
    )
  })
  const deployBtn = container.querySelector<HTMLButtonElement>('button.group')
  expect(deployBtn?.disabled).toBe(true)
})

it('blocks SettingsScreen from loading or saving while profile switch is blocked', async () => {
  ;(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {}
  invoke.mockResolvedValue({})

  await act(async () => {
    root!.render(
      <SettingsScreen
        username="PlayerUser"
        canWorkshop={false}
        profile="workshop"
        profileBlocked={true}
        profileError="Смена профиля в процессе..."
        onAccountChange={() => {}}
        tab="general"
        onTabChange={() => {}}
      />,
    )
  })

  // load_settings must NOT be called while profileBlocked is true
  const loadCalls = invoke.mock.calls.filter(([cmd]) => cmd === 'load_settings')
  expect(loadCalls.length).toBe(0)

  // Visible error is rendered
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    'Смена профиля в процессе...',
  )

  // Save button is disabled
  const saveBtn = Array.from(container.querySelectorAll('button')).find(
    (b) => b.textContent?.toUpperCase().includes('СОХРАНИТЬ'),
  )
  expect(saveBtn?.disabled).toBe(true)
})

it('handles live demotion of builder to player: hides custom selector, blocks actions, shows busy alert while game running, and automatically switches to game when game exits', async () => {
  ;(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {}

  let gamePhase: 'running' | 'idle' = 'running'
  let selectProfileCalledWith: string | null = null
  let role: 'builder' | 'player' = 'builder'

  const listeners: Record<string, Set<(event: { payload: unknown }) => void>> = {}
  listen.mockImplementation(async (event: string, handler: (event: { payload: unknown }) => void) => {
    if (!listeners[event]) listeners[event] = new Set()
    listeners[event]!.add(handler)
    return () => {
      listeners[event]?.delete(handler)
    }
  })

  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'load_settings') {
      return {
        activeProfile: 'workshop',
        workshopServer: 'workshop.blockfield.pro:25565',
        username: 'BuilderUser',
      }
    }
    if (cmd === 'account_status') {
      return { loggedIn: true, uuid: 'builder-uuid-1', username: 'BuilderUser' }
    }
    if (cmd === 'stats_player') {
      return { profile: { uuid: 'builder-uuid-1', role } }
    }
    if (cmd === 'game_status') {
      return { phase: gamePhase, revision: gamePhase === 'running' ? 1 : 2 }
    }
    if (cmd === 'select_profile') {
      selectProfileCalledWith = (args?.profile as string) ?? null
      if (gamePhase === 'running') {
        throw new Error('Закройте игру перед сменой профиля.')
      }
      return { activeProfile: args?.profile }
    }
    if (cmd === 'check_modpack_version') {
      return {
        localVersion: '1.0.0',
        remoteVersion: '1.0.0',
        needsUpdate: false,
        javaOk: true,
        loaderOk: true,
      }
    }
    return null
  })

  // 1. Initial boot: builder user with stored workshop profile
  await act(async () => {
    root!.render(<App />)
  })

  // Wait for initial render: role is builder, so custom profile select is visible
  await vi.waitFor(() => {
    expect(container.querySelector('#client-profile')).not.toBeNull()
  })
  expect(container.querySelector('#client-profile')?.textContent).toContain('Мастерская')

  const deployBtn = container.querySelector<HTMLButtonElement>('button.group')
  expect(deployBtn).not.toBeNull()

  // 2. Live demotion occurs: server role becomes 'player'
  role = 'player'
  // Trigger role refresh via window focus (simulating existing probe / focus polling path)
  await act(async () => {
    window.dispatchEvent(new Event('focus'))
  })

  // 3. Custom selector immediately disappears because player is not authorized for workshop
  await vi.waitFor(() => {
    expect(container.querySelector('#client-profile')).toBeNull()
  })

  // 4. select_profile('game') was attempted but game is running, so error was caught
  await vi.waitFor(() => {
    expect(selectProfileCalledWith).toBe('game')
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(
      'Закройте игру перед сменой профиля',
    )
  })

  // App is NOT killed or crashed. Deploy action remains disabled while blocked
  expect(deployBtn?.disabled).toBe(true)

  // 5. Game exits: transition game phase to 'idle'
  gamePhase = 'idle'
  await act(async () => {
    listeners['game://status']?.forEach((fn) =>
      fn({ payload: { phase: 'idle', revision: 2 } }),
    )
  })

  // 6. Automatic retry triggers select_profile('game') now that game is idle
  await vi.waitFor(() => {
    expect(container.querySelector('[role="alert"]')).toBeNull()
  })

  // Profile switch confirmed: selector remains hidden for player, actions unblocked
  expect(container.querySelector('#client-profile')).toBeNull()
})
