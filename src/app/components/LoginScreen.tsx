import { useState, type FormEvent } from 'react'
import { ArrowRight, Lock, User } from 'lucide-react'
import { Logo } from './Logo'
import { GridBackdrop, TopoBackdrop, CornerTicks } from './Backdrop'
import { useI18n } from '../i18n'
import { BRAND, COPYRIGHT } from '../constants'
import { contentText, localizedContentText, useLauncherContent } from '../../lib/content'
import { useLauncherVersion } from '../../lib/version'

export function LoginScreen({
  onSignIn,
  onRegister,
}: {
  onSignIn: (username: string, password: string, remember: boolean) => Promise<void>
  onRegister: (
    username: string,
    email: string,
    password: string,
    passwordConfirmation: string,
    remember: boolean,
  ) => Promise<void>
}) {
  const { lang, t } = useI18n()
  const content = useLauncherContent()
  const [remember, setRemember] = useState(true)
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [passwordConfirmation, setPasswordConfirmation] = useState('')
  const [registering, setRegistering] = useState(false)
  const [showResetHelp, setShowResetHelp] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const brand = contentText(content, 'brand') ?? BRAND
  const copyright = contentText(content, 'copyright') ?? COPYRIGHT
  const launcherVersion = useLauncherVersion()
  const sector =
    localizedContentText(content, lang, 'content.loginSector', 'loginSector', 'login_sector') ??
    t('login.sector')
  const slogan =
    localizedContentText(content, lang, 'content.loginSlogan', 'loginSlogan', 'login_slogan') ??
    t('login.slogan')

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    setSubmitting(true)
    setError('')
    setShowResetHelp(false)
    try {
      if (registering) {
        await onRegister(username, email, password, passwordConfirmation, remember)
      } else {
        await onSignIn(username, password, remember)
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="relative h-full w-full bg-[#070604] overflow-hidden">
      <TopoBackdrop />
      <GridBackdrop intensity={0.6} />

      <div className="absolute left-5 top-5 text-[10px] tracking-[0.16em] text-[#5E5040] flex flex-col gap-2">
        <span>{sector}</span>
      </div>
      <div className="absolute right-5 top-5 text-[10px] tracking-[0.16em] text-[#5E5040] flex flex-col items-end gap-2">
        <span>{t('login.build', { v: launcherVersion })}</span>
        <span className="flex items-center gap-2">{t('login.authRequired')}</span>
      </div>

      <div className="absolute bottom-5 left-5 right-5 flex items-end justify-between text-[10px] tracking-[0.16em] text-[#5E5040]">
        <span>{slogan}</span>
        <span>{copyright}</span>
      </div>

      <div className="h-full w-full grid place-items-center">
        <div className="relative w-[420px]">
          <CornerTicks />
          <div className="border border-[#2A2116] bg-[#11100D]/90 backdrop-blur-sm">
            <div className="px-9 pt-8 pb-7">
              <div className="flex flex-col items-center gap-5">
                <Logo size={44} />
                <div className="flex flex-col items-center gap-1.5">
                  <div className="tracking-[0.24em] text-[11px] text-[#C7AE86]">{brand}</div>
                  <h2 className="tracking-[0.18em] text-neutral-50">{t('login.launcher')}</h2>
                </div>
                <div className="h-px w-16 bg-gradient-to-r from-transparent via-[#8A571C] to-transparent" />
                <p className="text-[11px] tracking-[0.18em] text-[#8E7A5E]">
                  {t('login.authRequired')}
                </p>
              </div>

              <form className="mt-6 flex flex-col gap-3.5" onSubmit={handleSubmit}>
                <Field
                  icon={<User size={13} />}
                  label={t('login.callsign')}
                  value={username}
                  onChange={setUsername}
                  placeholder="operator@blockfield.gg"
                  name="username"
                  autoComplete="username"
                />
                {registering && (
                  <Field
                    icon={<User size={13} />}
                    label={t('login.email')}
                    value={email}
                    onChange={setEmail}
                    placeholder="operator@blockfield.gg"
                    type="email"
                    name="email"
                    autoComplete="email"
                  />
                )}
                <Field
                  icon={<Lock size={13} />}
                  label={t('login.accessKey')}
                  value={password}
                  onChange={setPassword}
                  placeholder="••••••••••••"
                  type="password"
                  name="password"
                  autoComplete={registering ? 'new-password' : 'current-password'}
                  minLength={registering ? 8 : undefined}
                />
                {registering && (
                  <Field
                    icon={<Lock size={13} />}
                    label={t('login.confirmKey')}
                    value={passwordConfirmation}
                    onChange={setPasswordConfirmation}
                    placeholder="••••••••"
                    type="password"
                    name="password_confirmation"
                    autoComplete="new-password"
                    minLength={8}
                  />
                )}

                <div className="flex items-center justify-between mt-1">
                  <label className="flex items-center gap-2 cursor-pointer group">
                    <input
                      type="checkbox"
                      checked={remember}
                      onChange={(event) => setRemember(event.target.checked)}
                      className="sr-only"
                    />
                    <span
                      className={`size-3.5 border flex items-center justify-center transition-colors ${
                        remember
                          ? 'border-[#F5A524] bg-[#F5A524]/15'
                          : 'border-[#3A2C1D] bg-transparent'
                      }`}
                    >
                      {remember && <span className="size-1.5 bg-[#F5A524]" />}
                    </span>
                    <span className="text-[10px] tracking-[0.22em] text-[#C7AE86] group-hover:text-neutral-200">
                      {t('login.remember')}
                    </span>
                  </label>
                  {!registering && (
                    <button
                      type="button"
                      onClick={() => setShowResetHelp(true)}
                      className="text-[10px] tracking-[0.22em] text-[#8E7A5E] hover:text-[#F5A524]"
                    >
                      {t('login.forgot')}
                    </button>
                  )}
                </div>

                <button
                  type="submit"
                  disabled={
                    submitting ||
                    !username.trim() ||
                    !password ||
                    (registering &&
                      (!email.trim() || password.length < 8 || password !== passwordConfirmation))
                  }
                  className="relative mt-3 h-11 group overflow-hidden border border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-all"
                  style={{
                    boxShadow:
                      'inset 0 0 0 1px rgba(245,165,36,0.08), 0 0 24px -8px rgba(245,165,36,0.4)',
                  }}
                >
                  <span className="absolute inset-0 bg-[#F5A524]/0 group-hover:bg-[#F5A524]/10 transition-colors" />
                  <span className="relative flex items-center justify-center gap-3 text-[12px] tracking-[0.24em] text-[#F3E7D0]">
                    {submitting
                      ? 'AUTHENTICATING…'
                      : registering
                        ? t('login.register')
                        : t('login.signIn')}
                    <ArrowRight size={14} />
                  </span>
                </button>
                {error && (
                  <p role="alert" className="text-[11px] text-[#E36A5D]">
                    {t(
                      error === 'Invalid credentials.'
                        ? 'login.invalidCredentials'
                        : error === 'Registration failed.'
                          ? 'login.registrationFailed'
                          : 'login.serviceUnavailable',
                    )}
                  </p>
                )}
                {showResetHelp && (
                  <p role="status" className="text-[10px] leading-relaxed text-[#C7AE86]">
                    {t('login.resetHelp')}
                  </p>
                )}
                <button
                  type="button"
                  onClick={() => {
                    setRegistering(!registering)
                    setError('')
                    setShowResetHelp(false)
                  }}
                  className="text-[10px] tracking-[0.2em] text-[#8E7A5E] hover:text-[#F5A524]"
                >
                  {registering ? t('login.haveAccount') : t('login.createAccount')}
                </button>
              </form>
            </div>

            <div className="border-t border-[#18130D] px-6 py-3 flex items-center justify-between bg-[#0B0906]">
              <div className="flex items-center gap-2">
                <span className="size-1.5 rounded-full bg-[#F5A524]" />
                <span className="text-[10px] tracking-[0.22em] text-[#8E7A5E]">
                  {t('login.ready')} · v{launcherVersion}
                </span>
              </div>
              <span className="text-[10px] tracking-[0.22em] text-[#5E5040]">AUTH REQUIRED</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  type = 'text',
  icon,
  name,
  autoComplete,
  minLength,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  placeholder?: string
  type?: string
  icon?: React.ReactNode
  name: string
  autoComplete: string
  minLength?: number
}) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[10px] tracking-[0.28em] text-[#8E7A5E]">{label}</span>
      <div className="flex items-center gap-2 h-10 border border-[#2A2116] bg-[#0B0906] px-3 focus-within:border-[#F5A524]/60 transition-colors">
        <span className="text-[#8E7A5E]">{icon}</span>
        <span className="h-4 w-px bg-[#2A2116]" />
        <input
          type={type}
          name={name}
          autoComplete={autoComplete}
          minLength={minLength}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="flex-1 bg-transparent outline-none text-[13px] text-neutral-100 placeholder:text-[#5E5040] tracking-wide"
        />
      </div>
    </label>
  )
}
