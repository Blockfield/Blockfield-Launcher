import { useState } from 'react'
import { ArrowRight, KeyRound, LoaderCircle, UserRound } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { ErrorDetail } from './ui-bits'
import { Setting } from './SettingsScreen'
import type { AccountStatus } from '../../lib/api'
import { friendlyError, type FriendlyError } from '../../lib/errors'
import { isValidUsername } from '../../lib/username'
import { useI18n } from '../i18n'

/** Login and registration; with `setPassword`, the one-time own password after account migration. */
export function LoginScreen({
  username = '',
  setPassword = false,
  onDone,
}: {
  username?: string
  setPassword?: boolean
  onDone: (account: AccountStatus) => void
}) {
  const { t } = useI18n()
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [name, setName] = useState(username)
  const [password, setPasswordText] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<FriendlyError | null>(null)
  const nameValid = setPassword || isValidUsername(name)
  const canSubmit = nameValid && password.length > 0 && !busy

  const run = async (request: () => Promise<AccountStatus>) => {
    setBusy(true)
    setError(null)
    try {
      onDone(await request())
    } catch (e) {
      setError(friendlyError(e, 'account'))
    } finally {
      setBusy(false)
    }
  }

  const submit = () => {
    if (!canSubmit) return
    void run(() =>
      setPassword
        ? invoke<AccountStatus>('change_password', { password })
        : invoke<AccountStatus>(mode, { username: name.trim(), password }),
    )
  }

  const inputClass = (invalid: boolean) =>
    `w-full h-10 border bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 outline-none transition-colors ${
      invalid
        ? 'border-[#c98b8b] focus:border-[#c98b8b]'
        : 'border-[#2A2116] focus:border-[#F5A524]/60'
    }`

  return (
    <div className="relative h-full w-full overflow-y-auto bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />

      <form
        className="screen-layout relative min-h-full flex flex-col items-center justify-center gap-4 px-4 py-6"
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <div className="w-full max-w-[640px] flex flex-col gap-4">
          <div className="shrink-0">
            <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
              {setPassword
                ? t('account.passwordTitle')
                : t(mode === 'login' ? 'account.title' : 'account.registerTitle')}
            </h1>
            <p className="mt-2 text-[11px] tracking-[0.1em] text-[#8E7A5E]">
              {t(setPassword ? 'account.passwordHint' : 'account.hint')}
            </p>
          </div>

          <div className="border border-[#2A2116] bg-[#11100D] px-3 md:px-5 py-2">
            {!setPassword && (
              <Setting
                icon={<UserRound size={14} />}
                label={t('account.username')}
                hint={t('account.usernameHint')}
              >
                <input
                  aria-label={t('account.username')}
                  aria-invalid={name.length > 0 && !nameValid}
                  autoFocus={!name}
                  autoComplete="username"
                  value={name}
                  maxLength={16}
                  onChange={(e) => setName(e.target.value)}
                  className={inputClass(name.length > 0 && !nameValid)}
                />
              </Setting>
            )}
            <Setting
              icon={<KeyRound size={14} />}
              label={t('account.password')}
              hint={t('account.passwordHintShort')}
            >
              <input
                type="password"
                aria-label={t('account.password')}
                autoFocus={setPassword || !!name}
                autoComplete={
                  mode === 'login' && !setPassword ? 'current-password' : 'new-password'
                }
                value={password}
                onChange={(e) => setPasswordText(e.target.value)}
                className={inputClass(false)}
              />
            </Setting>
          </div>

          {error && <ErrorDetail message={error.message} raw={error.raw} />}

          <div className="flex flex-wrap items-center justify-between gap-3">
            {setPassword ? (
              <button
                type="button"
                disabled={busy}
                onClick={() => void run(() => invoke<AccountStatus>('logout'))}
                className="text-[11px] text-[#C7AE86] underline underline-offset-4 hover:text-[#F3E7D0] disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-[#F5A524]"
              >
                {t('shell.logout')}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setMode(mode === 'login' ? 'register' : 'login')
                  setError(null)
                }}
                className="text-[11px] text-[#C7AE86] underline underline-offset-4 hover:text-[#F3E7D0] focus-visible:outline-2 focus-visible:outline-[#F5A524]"
              >
                {t(mode === 'login' ? 'account.toRegister' : 'account.toLogin')}
              </button>
            )}
            <button
              type="submit"
              disabled={!canSubmit}
              className={`h-10 px-6 flex items-center gap-3 border transition-colors ${
                canSubmit
                  ? 'border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524]'
                  : 'border-[#2A2116] bg-[#0B0906] opacity-50 cursor-not-allowed'
              }`}
            >
              {busy ? (
                <LoaderCircle size={13} className="animate-spin text-[#F3E7D0]" />
              ) : (
                <ArrowRight size={13} className="text-[#F3E7D0]" />
              )}
              <span className="text-[11px] tracking-[0.18em] text-[#F3E7D0]">
                {setPassword
                  ? t('account.save')
                  : t(mode === 'login' ? 'account.signIn' : 'account.register')}
              </span>
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
