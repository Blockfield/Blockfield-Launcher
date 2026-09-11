import { useEffect, useMemo, useState } from 'react'
import { WindowChrome } from './components/WindowChrome'
import { Shell } from './components/Shell'
import { MainScreen } from './components/MainScreen'
import { UpdateScreen } from './components/UpdateScreen'
import { SettingsScreen, type SettingsTab } from './components/SettingsScreen'
import { FirstRunScreen } from './components/FirstRunScreen'
import { I18nContext, translate } from './i18n'
import type { LauncherConfig } from '../lib/api'
import { checkModpack } from '../lib/modpack-check'
import { checkLauncherUpdate } from '../lib/launcher-update'
import { watchGameState } from '../lib/game-state'
import { listenLauncherStatus } from '../lib/events'
import { LoaderCircle } from 'lucide-react'
import { ErrorDetail } from './components/ui-bits'
import { friendlyError, type FriendlyError } from '../lib/errors'

type Screen = 'main' | 'update' | 'settings'

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

export default function App() {
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general')
  const [screen, setScreen] = useState<Screen>('main')
  const [username, setUsername] = useState('')
  const [updatesVisited, setUpdatesVisited] = useState(false)
  const [updateRequest, setUpdateRequest] = useState(0)
  const [commandError, setCommandError] = useState<string | null>(null)
  // First run: no username saved yet. Holding the loaded config here (instead of
  // just a boolean) lets the setup screen show real defaults (game dir/Java/RAM).
  const [firstRunConfig, setFirstRunConfig] = useState<LauncherConfig | null>(null)
  const [configError, setConfigError] = useState<FriendlyError | null>(null)
  const [configAttempt, setConfigAttempt] = useState(0)
  const [configLoaded, setConfigLoaded] = useState(!isTauri())

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

  // Offline identity: the username lives in launcher settings; no account service involved.
  useEffect(() => {
    if (!isTauri()) return
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<LauncherConfig>('load_settings'))
      .then((cfg) => {
        setUsername(cfg.username)
        checkModpack().catch((error) => console.error('Startup check failed:', error))
        if (!cfg.username) setFirstRunConfig(cfg)
      })
      .catch((e) => setConfigError(friendlyError(e, 'settings-load')))
      .finally(() => setConfigLoaded(true))
  }, [configAttempt])

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

  if (firstRunConfig) {
    return (
      <I18nContext.Provider value={i18n}>
        <WindowChrome>
          <FirstRunScreen
            defaults={firstRunConfig}
            onComplete={(config) => {
              setUsername(config.username)
              setFirstRunConfig(null)
            }}
          />
        </WindowChrome>
      </I18nContext.Provider>
    )
  }

  return (
    <I18nContext.Provider value={i18n}>
      <WindowChrome>
        <Shell
          user={{ username: username || '—', role: 'игрок' }}
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
              onPlay={() => {
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
          {screen === 'settings' && (
            <SettingsScreen
              username={username}
              onUsernameSaved={setUsername}
              tab={settingsTab}
              onTabChange={setSettingsTab}
            />
          )}
        </Shell>
      </WindowChrome>
    </I18nContext.Provider>
  )
}
