import { useCallback, useEffect, useMemo, useState } from 'react'
import { WindowChrome } from './components/WindowChrome'
import { LoginScreen } from './components/LoginScreen'
import { Shell } from './components/Shell'
import { MainScreen } from './components/MainScreen'
import { UpdateScreen } from './components/UpdateScreen'
import { SettingsScreen } from './components/SettingsScreen'
import { I18nContext, makeT, type Lang } from './i18n'
import { contentTranslations, useLauncherContent } from '../lib/content'

type Screen = 'login' | 'main' | 'update' | 'settings'

const LANG_KEY = 'blockfield.lang'

const loadLang = (): Lang => {
  if (typeof localStorage === 'undefined') return 'en'
  const saved = localStorage.getItem(LANG_KEY)
  return saved === 'ru' || saved === 'uk' || saved === 'en' ? saved : 'en'
}

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

export default function App() {
  const content = useLauncherContent()
  const [screen, setScreen] = useState<Screen>('login')
  const [lang, setLangState] = useState<Lang>(loadLang)

  const setLang = useCallback((next: Lang) => {
    setLangState(next)
    if (typeof localStorage !== 'undefined') localStorage.setItem(LANG_KEY, next)
    if (typeof document !== 'undefined') document.documentElement.lang = next
  }, [])

  const i18n = useMemo(
    () => ({ lang, setLang, t: makeT(lang, contentTranslations(content, lang)) }),
    [content, lang, setLang],
  )

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
            // User will need to restart the launcher manually
            console.log('Update installed — launcher will restart on next launch')
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
      <WindowChrome key={lang}>
        {screen === 'login' ? (
          <LoginScreen onSignIn={() => setScreen('main')} />
        ) : (
          <Shell
            active={screen as Exclude<Screen, 'login'>}
            onNavigate={setScreen}
            onLogout={() => setScreen('login')}
          >
            {screen === 'main' && <MainScreen onPlay={() => setScreen('update')} />}
            {screen === 'update' && <UpdateScreen />}
            {screen === 'settings' && <SettingsScreen onLogout={() => setScreen('login')} />}
          </Shell>
        )}
      </WindowChrome>
    </I18nContext.Provider>
  )
}
