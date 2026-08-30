import { useCallback, useEffect, useMemo, useState } from 'react'
import { WindowChrome } from './components/WindowChrome'
import { Shell } from './components/Shell'
import { MainScreen } from './components/MainScreen'
import { UpdateScreen } from './components/UpdateScreen'
import { SettingsScreen } from './components/SettingsScreen'
import { I18nContext, makeT, type Lang } from './i18n'
import type { LauncherConfig } from '../lib/api'

type Screen = 'main' | 'update' | 'settings'

const LANG_KEY = 'blockfield.lang'

const loadLang = (): Lang => {
  if (typeof localStorage === 'undefined') return 'en'
  const saved = localStorage.getItem(LANG_KEY)
  return saved === 'ru' || saved === 'uk' || saved === 'en' ? saved : 'en'
}

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

export default function App() {
  const [screen, setScreen] = useState<Screen>('main')
  const [lang, setLangState] = useState<Lang>(loadLang)
  const [username, setUsername] = useState('')

  const setLang = useCallback((next: Lang) => {
    setLangState(next)
    if (typeof localStorage !== 'undefined') localStorage.setItem(LANG_KEY, next)
    if (typeof document !== 'undefined') document.documentElement.lang = next
  }, [])

  const i18n = useMemo(() => ({ lang, setLang, t: makeT(lang) }), [lang, setLang])

  // Offline identity: the username lives in launcher settings; no account service involved.
  useEffect(() => {
    if (!isTauri()) return
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<LauncherConfig>('load_settings'))
      .then((cfg) => {
        setUsername(cfg.username)
        if (!cfg.username) setScreen('settings')
      })
      .catch((e) => console.error('Failed to load settings:', e))
  }, [])

  // Check for launcher updates on mount
  useEffect(() => {
    if (!isTauri()) return

    const checkLauncherUpdate = async () => {
      try {
        const { check } = await import('@tauri-apps/plugin-updater')
        const update = await check()

        if (update?.available) {
          const { ask } = await import('@tauri-apps/plugin-dialog')
          const shouldUpdate = await ask(
            `Launcher update available: ${update.currentVersion} → ${update.version}\n\n${update.body ?? ''}`,
            { title: 'Launcher Update', kind: 'info' },
          )

          if (shouldUpdate) {
            await update.downloadAndInstall()
            const { relaunch } = await import('@tauri-apps/plugin-process')
            await relaunch()
          }
        }
      } catch (e) {
        // Silently fail — the updater endpoint may not be deployed yet
        console.log('Launcher update check skipped:', e)
      }
    }

    checkLauncherUpdate()
  }, [])

  return (
    <I18nContext.Provider value={i18n}>
      <WindowChrome>
        <Shell user={{ username: username || '—', role: 'operator' }} active={screen} onNavigate={setScreen}>
          {screen === 'main' && <MainScreen onPlay={() => setScreen('update')} />}
          {screen === 'update' && <UpdateScreen />}
          {screen === 'settings' && (
            <SettingsScreen username={username} onUsernameSaved={setUsername} />
          )}
        </Shell>
      </WindowChrome>
    </I18nContext.Provider>
  )
}
