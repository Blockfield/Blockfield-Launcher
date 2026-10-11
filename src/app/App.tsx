import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { WindowChrome } from './components/WindowChrome'
import { Shell } from './components/Shell'
import { MainScreen } from './components/MainScreen'
import { UpdateScreen } from './components/UpdateScreen'
import { SettingsScreen, type SettingsTab } from './components/SettingsScreen'
import { StatsScreen } from './components/StatsScreen'
import { FirstRunScreen } from './components/FirstRunScreen'
import { LoginScreen } from './components/LoginScreen'
import { I18nContext, translate } from './i18n'
import type { AccountStatus, LauncherConfig, VersionCheckResult } from '../lib/api'
import { hasWorkshopAccess } from '../lib/workshop'
import {
  checkModpack,
  invalidateModpackCheck,
  getModpackVersionSnapshot,
} from '../lib/modpack-check'
import { checkLauncherUpdate } from '../lib/launcher-update'
import { useGameState, watchGameState } from '../lib/game-state'
import { listenLauncherStatus } from '../lib/events'
import { LoaderCircle } from 'lucide-react'
import { ErrorDetail } from './components/ui-bits'
import { friendlyError, type FriendlyError } from '../lib/errors'
import { loadStats, type PlayerStats } from '../lib/stats'

type Screen = 'main' | 'update' | 'stats' | 'settings'

const ROLE_LABELS: Record<string, string> = {
  builder: 'строитель',
  moderator: 'модератор',
  admin: 'администратор',
  owner: 'владелец',
}

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

export default function App() {
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general')
  const [screen, setScreen] = useState<Screen>('main')
  const [account, setAccount] = useState<AccountStatus | null>(null)
  const [accountLoaded, setAccountLoaded] = useState(!isTauri())
  const [staff, setStaff] = useState<{ uuid: string; role: string | null } | null>(null)
  const uuid = account?.loggedIn ? account.uuid : null

  const refreshRole = useCallback(async (id: string) => {
    try {
      const result = await loadStats<PlayerStats>({ kind: 'player', uuid: id, query: {} })
      // A failed lookup (no data and not stale) must not look like "player":
      // keep the previous role so a transient error never triggers the
      // workshop->game auto-downgrade (which persists activeProfile to disk).
      if (result.stale || !result.error)
        setStaff({ uuid: id, role: result.data?.profile.role ?? null })
    } catch {
      setStaff((previous) => (previous?.uuid === id ? previous : { uuid: id, role: null }))
    }
  }, [])

  useEffect(() => {
    if (!uuid) return
    let active = true
    queueMicrotask(() => {
      if (active) void refreshRole(uuid)
    })
    return () => {
      active = false
    }
  }, [uuid, refreshRole])

  const handleAccountChange = useCallback(
    (acc: AccountStatus) => {
      setAccount(acc)
      if (!acc.loggedIn) {
        setStaff(null)
      } else {
        void refreshRole(acc.uuid)
      }
    },
    [refreshRole],
  )

  const gameState = useGameState()
  const gameBusy = gameState.phase !== 'idle'

  const [updatesVisited, setUpdatesVisited] = useState(false)
  const [updateRequest, setUpdateRequest] = useState(0)
  const [commandError, setCommandError] = useState<string | null>(null)
  // First run: never signed in on this install. Holding the loaded config here (instead of
  // just a boolean) lets the setup screen show real defaults (game dir/Java/RAM).
  const [firstRunConfig, setFirstRunConfig] = useState<LauncherConfig | null>(null)
  const [configError, setConfigError] = useState<FriendlyError | null>(null)
  const [configAttempt, setConfigAttempt] = useState(0)
  const [configLoaded, setConfigLoaded] = useState(!isTauri())
  const [profile, setProfile] = useState<'game' | 'workshop'>('game')
  const [workshopServer, setWorkshopServer] = useState('')
  const [profileChanging, setProfileChanging] = useState(false)
  const [profileError, setProfileError] = useState<string | null>(null)
  const [pendingProfileRetry, setPendingProfileRetry] = useState<'game' | 'workshop' | null>(null)

  const currentRole = account?.loggedIn && staff?.uuid === account.uuid ? staff.role : null
  const canWorkshop = hasWorkshopAccess(currentRole)

  const authResolved = accountLoaded && (!account?.loggedIn || staff?.uuid === uuid)
  const profileBlocked =
    profileChanging || (!canWorkshop && profile === 'workshop') || profileError !== null

  const selectProfile = useCallback(
    async (next: 'game' | 'workshop') => {
      if (next === 'workshop' && !canWorkshop) return
      if (next === profile && !profileError) return
      setProfileChanging(true)
      setProfileError(null)
      try {
        const config = await invoke<LauncherConfig>('select_profile', { profile: next })
        invalidateModpackCheck()
        setUpdatesVisited(false)
        setProfile(config.activeProfile ?? next)
        setWorkshopServer(config.workshopServer ?? '')
        setPendingProfileRetry(null)
      } catch (e) {
        const message = friendlyError(e, 'select-profile').message
        setProfileError(message)
        setPendingProfileRetry(next)
      } finally {
        setProfileChanging(false)
      }
    },
    [canWorkshop, profile, profileError],
  )

  useEffect(() => {
    if (
      !configLoaded ||
      !authResolved ||
      canWorkshop ||
      profile !== 'workshop' ||
      profileChanging ||
      profileError
    )
      return
    let active = true
    queueMicrotask(() => {
      if (active) void selectProfile('game')
    })
    return () => {
      active = false
    }
  }, [
    configLoaded,
    authResolved,
    canWorkshop,
    profile,
    profileChanging,
    profileError,
    selectProfile,
  ])

  const prevGameBusy = useRef(gameBusy)
  useEffect(() => {
    const wasBusy = prevGameBusy.current
    prevGameBusy.current = gameBusy
    if (wasBusy && !gameBusy && pendingProfileRetry && !profileChanging) {
      void selectProfile(canWorkshop ? pendingProfileRetry : 'game')
    }
  }, [gameBusy, pendingProfileRetry, profileChanging, canWorkshop, selectProfile])

  useEffect(() => {
    if (!isTauri()) return
    const subscription = listen('room://pending', () =>
      setScreen((current) => (current === 'update' ? current : 'main')),
    )
    return () => {
      void subscription.then((stop) => stop())
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    const subscription = listen('account://expired', () => {
      setAccount((current) => current && { ...current, loggedIn: false })
      setStaff(null)
    })
    return () => {
      void subscription.then((stop) => stop())
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    const subscription = watchGameState()
    void subscription.catch((error) => console.error('Failed to read game state:', error))
    return () => {
      void subscription.then((stop) => stop()).catch(() => {})
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    const subscription = listenLauncherStatus((status) => {
      if (status.phase === 'hook-error') setCommandError(status.message)
    })
    return () => {
      void subscription.then((unlisten) => unlisten())
    }
  }, [])
  const navigate = (next: Screen) => {
    if (next === 'update') setUpdatesVisited(true)
    setScreen(next)
  }

  const i18n = useMemo(() => ({ lang: 'ru' as const, t: translate }), [])

  useEffect(() => {
    if (!isTauri()) return
    import('@tauri-apps/api/core')
      .then(({ invoke }) =>
        Promise.all([
          invoke<LauncherConfig>('load_settings'),
          invoke<AccountStatus>('account_status'),
        ]),
      )
      .then(([cfg, status]) => {
        setProfile(cfg.activeProfile ?? 'game')
        setWorkshopServer(cfg.workshopServer ?? '')
        setAccount(status)
        setAccountLoaded(true)
        checkModpack().catch((error) => console.error('Startup check failed:', error))
        if (!cfg.username) setFirstRunConfig(cfg)
      })
      .catch((e) => setConfigError(friendlyError(e, 'settings-load')))
      .finally(() => setConfigLoaded(true))
  }, [configAttempt])

  // Single timer/focus probe for the shared pack-version store (src/lib/modpack-check.ts).
  // Runs regardless of which tab is active, so "Играть" and "Обновления" always converge
  // after a background check instead of each screen polling (and caching) independently.
  useEffect(() => {
    if (!isTauri()) return
    const probe = () => {
      if (uuid) {
        void refreshRole(uuid)
      }
      const current = getModpackVersionSnapshot()
      if (!current) return
      void invoke<VersionCheckResult>('check_modpack_version')
        .then((result) => {
          if (
            result.remoteVersion !== current.remoteVersion ||
            result.needsUpdate !== current.needsUpdate
          ) {
            invalidateModpackCheck()
            checkModpack().catch((error) => console.error('Version re-check failed:', error))
          }
        })
        .catch(() => {})
    }
    const interval = window.setInterval(probe, 60_000)
    window.addEventListener('focus', probe)
    return () => {
      window.clearInterval(interval)
      window.removeEventListener('focus', probe)
    }
  }, [uuid, refreshRole])

  // Check for launcher updates on mount
  useEffect(() => {
    if (!isTauri()) return

    checkLauncherUpdate().catch((error) => console.log('Launcher update check skipped:', error))
  }, [])

  if (!configLoaded) {
    return (
      <I18nContext.Provider value={i18n}>
        <WindowChrome>
          <div className="h-full w-full grid place-items-center bg-[#070604]">
            <LoaderCircle size={24} className="animate-spin text-[#8E7A5E]" />
          </div>
        </WindowChrome>
      </I18nContext.Provider>
    )
  }

  if (configError) {
    return (
      <I18nContext.Provider value={i18n}>
        <WindowChrome>
          <div className="h-full grid place-items-center bg-[#070604] p-6">
            <div className="max-w-lg space-y-4">
              <ErrorDetail message={configError.message} raw={configError.raw} />
              <button
                type="button"
                className="border border-[#F5A524]/40 px-4 py-3 text-[12px] text-[#F3E7D0] focus-visible:outline-2 focus-visible:outline-[#F5A524]"
                onClick={() => {
                  setConfigLoaded(false)
                  setConfigError(null)
                  setConfigAttempt((attempt) => attempt + 1)
                }}
              >
                Повторить
              </button>
            </div>
          </div>
        </WindowChrome>
      </I18nContext.Provider>
    )
  }

  if (!account?.loggedIn || account.needsPassword) {
    return (
      <I18nContext.Provider value={i18n}>
        <WindowChrome>
          <LoginScreen
            key={account?.needsPassword ? 'password' : 'login'}
            username={account?.username}
            setPassword={account?.needsPassword}
            onDone={handleAccountChange}
          />
        </WindowChrome>
      </I18nContext.Provider>
    )
  }

  if (firstRunConfig) {
    return (
      <I18nContext.Provider value={i18n}>
        <WindowChrome>
          <FirstRunScreen defaults={firstRunConfig} onComplete={() => setFirstRunConfig(null)} />
        </WindowChrome>
      </I18nContext.Provider>
    )
  }

  return (
    <I18nContext.Provider value={i18n}>
      <WindowChrome>
        <Shell
          serverAddress={!profileBlocked && profile === 'workshop' ? workshopServer : undefined}
          user={{
            username: account?.username || '—',
            role: ROLE_LABELS[currentRole || ''] ?? 'игрок',
          }}
          active={screen}
          onNavigate={navigate}
          onLauncherUpdate={() => {
            setSettingsTab('launcher')
            navigate('settings')
          }}
        >
          {commandError && (
            <div
              role="alert"
              className="absolute bottom-4 inset-x-4 z-50 flex items-start gap-4 border border-[#c98b8b] bg-[#11100D] p-4 text-[12px] text-[#F3E7D0]"
            >
              <span className="min-w-0 flex-1 break-words">
                Ошибка команды после выхода: {commandError}
              </span>
              <button
                onClick={() => setCommandError(null)}
                className="shrink-0 underline underline-offset-4"
              >
                Закрыть
              </button>
            </div>
          )}
          {screen === 'main' && (
            <MainScreen
              key={profile}
              profile={profile}
              profileChanging={profileChanging}
              profileBlocked={profileBlocked}
              profileError={profileError}
              onSelectProfile={canWorkshop ? (next) => void selectProfile(next) : undefined}
              onPlay={() => {
                if (profileBlocked) return
                setUpdateRequest((value) => value + 1)
                navigate('update')
              }}
            />
          )}
          {updatesVisited && (
            <div hidden={screen !== 'update'} className="h-full min-h-0">
              <UpdateScreen updateRequest={updateRequest} />
            </div>
          )}
          {screen === 'stats' && <StatsScreen uuid={account.uuid} />}
          {screen === 'settings' && (
            <SettingsScreen
              username={account?.username ?? ''}
              canWorkshop={canWorkshop}
              profile={profile}
              profileBlocked={profileBlocked}
              profileError={profileError}
              onWorkshopServerSaved={setWorkshopServer}
              onAccountChange={handleAccountChange}
              tab={settingsTab}
              onTabChange={setSettingsTab}
            />
          )}
        </Shell>
      </WindowChrome>
    </I18nContext.Provider>
  )
}
