import { useCallback, useEffect, useMemo, useState } from 'react'
import { WindowChrome } from './components/WindowChrome'
import { LoginScreen } from './components/LoginScreen'
import { Shell } from './components/Shell'
import { MainScreen } from './components/MainScreen'
import { UpdateScreen } from './components/UpdateScreen'
import { SettingsScreen } from './components/SettingsScreen'
import { I18nContext, makeT, type Lang } from './i18n'
import { contentTranslations, useLauncherContent } from '../lib/content'
import { login, logout, register, restoreSession, type AuthSession } from '../lib/auth'

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
  const [session, setSession] = useState<AuthSession | null>(null)
  const [restoringSession, setRestoringSession] = useState(true)

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

  const syncGameIdentity = useCallback(async (auth: AuthSession | null) => {
    if (!isTauri()) return
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_game_identity', {
      identity: auth
        ? {
            username: auth.user.username,
            uuid: auth.user.minecraft_uuid,
            accessToken: auth.accessToken,
          }
        : null,
    })
  }, [])

  useEffect(() => {
    restoreSession()
      .then(async (auth) => {
        setSession(auth)
        await syncGameIdentity(auth)
        if (auth) setScreen('main')
      })
      .finally(() => setRestoringSession(false))
  }, [syncGameIdentity])

  useEffect(() => {
    if (!session) return
    const refresh = async () => {
      const auth = await restoreSession()
      if (!auth) {
        setSession(null)
        setScreen('login')
      } else if (auth.accessToken !== session.accessToken) {
        setSession(auth)
        await syncGameIdentity(auth)
      }
    }
    const interval = window.setInterval(refresh, 5 * 60 * 1000)
    window.addEventListener('focus', refresh)
    return () => {
      window.clearInterval(interval)
      window.removeEventListener('focus', refresh)
    }
  }, [session, syncGameIdentity])

  const handleSignIn = useCallback(
    async (username: string, password: string, remember: boolean) => {
      const auth = await login(username, password, remember)
      await syncGameIdentity(auth)
      setSession(auth)
      setScreen('main')
    },
    [syncGameIdentity],
  )

  const handleRegister = useCallback(
    async (
      username: string,
      email: string,
      password: string,
      passwordConfirmation: string,
      remember: boolean,
    ) => {
      const auth = await register(username, email, password, passwordConfirmation, remember)
      await syncGameIdentity(auth)
      setSession(auth)
      setScreen('main')
    },
    [syncGameIdentity],
  )

  const handleLogout = useCallback(async () => {
    const current = session
    setSession(null)
    setScreen('login')
    await syncGameIdentity(null)
    await logout(current)
  }, [session, syncGameIdentity])

  return (
    <I18nContext.Provider value={i18n}>
      <WindowChrome>
        {restoringSession ? null : screen === 'login' || !session ? (
          <LoginScreen onSignIn={handleSignIn} onRegister={handleRegister} />
        ) : (
          <Shell
            user={session.user}
            active={screen as Exclude<Screen, 'login'>}
            onNavigate={setScreen}
            onLogout={handleLogout}
          >
            {screen === 'main' && <MainScreen onPlay={() => setScreen('update')} />}
            {screen === 'update' && <UpdateScreen />}
            {screen === 'settings' && (
              <SettingsScreen onLogout={handleLogout} username={session.user.username} />
            )}
          </Shell>
        )}
      </WindowChrome>
    </I18nContext.Provider>
  )
}
