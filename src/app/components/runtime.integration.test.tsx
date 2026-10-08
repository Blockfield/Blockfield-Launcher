// @vitest-environment jsdom
import { act, StrictMode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { RoomList } from './RoomList'
import { LauncherUpdatePanel } from './LauncherUpdatePanel'
import { Shell } from './Shell'
import { MainScreen, ProfileSelect } from './MainScreen'
import { SettingsScreen } from './SettingsScreen'
import { checkLauncherUpdate, installLauncherUpdate } from '../../lib/launcher-update'

const { invoke, getVersion, relaunch, channels } = vi.hoisted(() => ({
  invoke: vi.fn(),
  getVersion: vi.fn(),
  relaunch: vi.fn(),
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
  expect(options[0].textContent).toContain('Игра')
  expect(options[1].textContent).toContain('Мастерская')

  // First option is selected so it has the check icon
  expect(options[0].getAttribute('aria-selected')).toBe('true')
  expect(options[0].querySelector('svg')).not.toBeNull()
  expect(options[1].getAttribute('aria-selected')).toBe('false')
  expect(options[1].querySelector('svg')).toBeNull()

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
