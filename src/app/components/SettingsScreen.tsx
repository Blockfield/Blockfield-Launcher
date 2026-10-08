import { ramLimitGb, clampRamGb } from '../../lib/ram'
import { useCallback, useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import {
  Folder,
  Gamepad2,
  Cpu,
  Coffee,
  RefreshCw,
  UserRound,
  Save,
  Minus,
  Plus,
  LoaderCircle,
  Terminal,
  EyeOff,
  Shirt,
  Upload,
  LogOut,
  KeyRound,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { LauncherUpdatePanel } from './LauncherUpdatePanel'
import { DevPanel } from './DevPanel'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { ErrorDetail } from './ui-bits'
import { useI18n } from '../i18n'
import type {
  AccountStatus,
  LauncherConfig,
  SkinPreview as SkinPreviewData,
  SkinUploadResult,
} from '../../lib/api'
import { SkinPreview } from './SkinPreview'
import { forgetFace } from '../../lib/player-face'
import { invalidateModpackCheck } from '../../lib/modpack-check'
import { localizedContentText, useLauncherContent } from '../../lib/content'
import { friendlyError, type FriendlyError } from '../../lib/errors'
import { selectedGameDirectory } from '../../lib/game-directory'

/** Detect whether we're running inside Tauri. */
const isTauri = () => '__TAURI_INTERNALS__' in window

export type SettingsTab = 'general' | 'runtime' | 'commands' | 'skin' | 'launcher'

export function SettingsScreen({
  username,
  canWorkshop = false,
  profile: initialProfile = 'game',
  onAccountChange,
  tab,
  onTabChange: setTab,
  onWorkshopServerSaved,
}: {
  username: string
  canWorkshop?: boolean
  profile?: 'game' | 'workshop'
  onAccountChange: (account: AccountStatus) => void
  tab: SettingsTab
  onTabChange: (tab: SettingsTab) => void
  onWorkshopServerSaved?: (address: string) => void
}) {
  const { lang, t } = useI18n()
  const content = useLauncherContent()
  const [dir, setDir] = useState('')
  const [java, setJava] = useState('')
  const [profile, setProfile] = useState<'game' | 'workshop'>(canWorkshop ? initialProfile : 'game')
  const isWorkshop = canWorkshop && profile === 'workshop'
  const [workshopServer, setWorkshopServer] = useState('')
  const [ram, setRam] = useState(4)
  const [maxRam, setMaxRam] = useState(32)
  const [preLaunchCommand, setPreLaunchCommand] = useState('')
  const [postExitCommand, setPostExitCommand] = useState('')
  const [discordPresence, setDiscordPresence] = useState(true)
  const [hideWhilePlaying, setHideWhilePlaying] = useState(false)
  const [autoUpdate, setAutoUpdate] = useState(true)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saveMessage, setSaveMessage] = useState<string | null>(null)
  const [saveErrorRaw, setSaveErrorRaw] = useState<string | null>(null)
  const [skinPath, setSkinPath] = useState('')
  const [capePath, setCapePath] = useState('')
  const [slim, setSlim] = useState(false)
  const [skinBusy, setSkinBusy] = useState(false)
  const [skinMessage, setSkinMessage] = useState<string | null>(null)
  const [skinErrorRaw, setSkinErrorRaw] = useState<string | null>(null)
  const [skinPreview, setSkinPreview] = useState<SkinPreviewData | null>(null)
  const [passwordOpen, setPasswordOpen] = useState(false)
  const [newPassword, setNewPassword] = useState('')
  const [accountBusy, setAccountBusy] = useState(false)
  const [accountError, setAccountError] = useState<FriendlyError | null>(null)
  const [passwordChanged, setPasswordChanged] = useState(false)
  const preferences =
    localizedContentText(
      content,
      lang,
      'content.settingsPreferences',
      'settingsPreferences',
      'settings_preferences',
    ) ?? t('settings.preferences')

  // Load settings on mount
  useEffect(() => {
    if (!isTauri()) {
      queueMicrotask(() => setLoading(false))
      return
    }

    invoke<LauncherConfig>('load_settings')
      .then((cfg) => {
        setProfile(canWorkshop ? (cfg.activeProfile ?? 'game') : 'game')
        setWorkshopServer(cfg.workshopServer ?? '')
        setDir(cfg.gameDir)
        setJava(cfg.javaPath)
        setMaxRam(ramLimitGb(cfg.maxRamMb))
        setRam(clampRamGb(cfg.ramMb / 1024, ramLimitGb(cfg.maxRamMb)))
        setAutoUpdate(cfg.autoUpdate)
        setHideWhilePlaying(cfg.hideWhilePlaying ?? false)
        setDiscordPresence(cfg.discordPresence ?? true)
        setPreLaunchCommand(cfg.preLaunchCommand ?? '')
        setPostExitCommand(cfg.postExitCommand ?? '')
      })
      .catch((e) => console.error('Failed to load settings:', e))
      .finally(() => setLoading(false))
  }, [canWorkshop])

  useEffect(() => {
    if (!canWorkshop && profile === 'workshop') {
      setProfile('game')
    } else if (canWorkshop && initialProfile) {
      setProfile(initialProfile)
    }
  }, [canWorkshop, initialProfile, profile])

  const markDirty = useCallback(() => setDirty(true), [])

  const handleSave = useCallback(async () => {
    if (!isTauri()) return
    if (maxRam < 2) {
      setSaveMessage('Недостаточно памяти: для игры нужно выделить минимум 2 ГБ.')
      setSaveErrorRaw(null)
      return
    }
    setSaving(true)
    setSaveMessage(null)
    setSaveErrorRaw(null)
    try {
      await invoke('save_settings', {
        config: {
          activeProfile: isWorkshop ? 'workshop' : 'game',
          workshopServer,
          gameDir: dir,
          javaPath: java,
          ramMb: ram * 1024,
          autoUpdate,
          hideWhilePlaying,
          discordPresence,
          preLaunchCommand,
          postExitCommand,
          lang,
        } satisfies LauncherConfig,
      })
      invalidateModpackCheck()
      onWorkshopServerSaved?.(workshopServer)
      setDirty(false)
      setSaveMessage(t('settings.saved'))
      setTimeout(() => setSaveMessage(null), 3000)
    } catch (e) {
      console.error('Failed to save settings:', e)
      const friendly = friendlyError(e, 'settings-save')
      setSaveMessage(friendly.message)
      setSaveErrorRaw(friendly.raw)
    } finally {
      setSaving(false)
    }
  }, [
    profile,
    workshopServer,
    onWorkshopServerSaved,
    dir,
    java,
    ram,
    autoUpdate,
    hideWhilePlaying,
    discordPresence,
    preLaunchCommand,
    postExitCommand,
    lang,
    t,
    maxRam,
  ])

  const handleReset = useCallback(() => {
    invoke<LauncherConfig>('load_settings')
      .then((cfg) => {
        setProfile(canWorkshop ? (cfg.activeProfile ?? 'game') : 'game')
        setWorkshopServer(cfg.workshopServer ?? '')
        setDir(cfg.gameDir)
        setJava(cfg.javaPath)
        setMaxRam(ramLimitGb(cfg.maxRamMb))
        setRam(clampRamGb(cfg.ramMb / 1024, ramLimitGb(cfg.maxRamMb)))
        setAutoUpdate(cfg.autoUpdate)
        setHideWhilePlaying(cfg.hideWhilePlaying ?? false)
        setDiscordPresence(cfg.discordPresence ?? true)
        setPreLaunchCommand(cfg.preLaunchCommand ?? '')
        setPostExitCommand(cfg.postExitCommand ?? '')
      })
      .catch(console.error)
    setDirty(false)
  }, [])

  useEffect(() => {
    if (tab !== 'skin' || !isTauri()) return
    let cancelled = false
    const timer = setTimeout(() => {
      invoke<SkinPreviewData>('preview_skin', {
        username,
        skinPath: skinPath || null,
        capePath: capePath || null,
        slim,
      })
        .then((preview) => {
          if (!cancelled) setSkinPreview(preview)
        })
        .catch(console.error)
    }, 300)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [tab, username, skinPath, capePath, slim])

  const handleSkinUpload = useCallback(async () => {
    if (!isTauri()) return
    setSkinBusy(true)
    setSkinMessage(null)
    setSkinErrorRaw(null)
    try {
      await invoke<SkinUploadResult>('upload_skin', {
        skinPath: skinPath || null,
        capePath: capePath || null,
        slim,
      })
      forgetFace(username)
      setSkinMessage(t('settings.skinUploaded'))
    } catch (e) {
      const friendly = friendlyError(e, 'skin-upload')
      setSkinMessage(friendly.message)
      setSkinErrorRaw(friendly.raw)
    } finally {
      setSkinBusy(false)
    }
  }, [username, skinPath, capePath, slim, t])

  const handleAccount = useCallback(
    async (command: 'logout' | 'change_password') => {
      setAccountBusy(true)
      setAccountError(null)
      setPasswordChanged(false)
      try {
        const account = await invoke<AccountStatus>(
          command,
          command === 'change_password' ? { password: newPassword } : undefined,
        )
        if (command === 'change_password') {
          setNewPassword('')
          setPasswordOpen(false)
          setPasswordChanged(true)
        } else if (command === 'logout') {
          setProfile('game')
        }
        onAccountChange(account)
      } catch (e) {
        setAccountError(friendlyError(e, 'account'))
      } finally {
        setAccountBusy(false)
      }
    },
    [newPassword, onAccountChange],
  )

  const handleBrowsePng = useCallback(async (set: (path: string) => void) => {
    if (!isTauri()) return
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({ filters: [{ name: 'PNG', extensions: ['png'] }] })
      if (selected) set(selected as string)
    } catch (e) {
      console.error('Browse failed:', e)
    }
  }, [])

  const handleBrowse = useCallback(
    async (field: 'dir' | 'java') => {
      if (!isTauri()) return
      try {
        const { open } = await import('@tauri-apps/plugin-dialog')
        const selected = await open({
          directory: field === 'dir',
          title: field === 'dir' ? 'Select Game Directory' : 'Select Java Executable',
        })
        if (selected) {
          const path = selected as string
          if (field === 'dir') setDir(await selectedGameDirectory(path))
          else setJava(path)
          markDirty()
        }
      } catch (e) {
        console.error('Browse failed:', e)
      }
    },
    [markDirty],
  )

  if (loading) {
    return (
      <div className="relative h-full w-full overflow-hidden bg-[#070604] flex items-center justify-center">
        <LoaderCircle size={24} className="animate-spin text-[#8E7A5E]" />
      </div>
    )
  }

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />

      <div className="settings-layout screen-layout relative h-full min-h-0 flex flex-col gap-4 overflow-hidden">
        <div className="shrink-0 flex flex-wrap items-baseline gap-3">
          <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
            {t('nav.settings')}
          </h1>
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">{preferences}</span>
        </div>

        <div className="min-h-0 flex-1 flex flex-col border border-[#2A2116] bg-[#11100D] overflow-hidden">
          <div
            role="tablist"
            aria-label={t('nav.settings')}
            className="shrink-0 grid grid-cols-2 md:grid-cols-5 border-b border-[#2A2116] bg-[#0B0906]"
          >
            {(['general', 'runtime', 'commands', 'skin', 'launcher'] as const).map(
              (id, index, tabs) => (
                <button
                  key={id}
                  id={`settings-tab-${id}`}
                  role="tab"
                  aria-selected={tab === id}
                  aria-controls={`settings-panel-${id}`}
                  tabIndex={tab === id ? 0 : -1}
                  onClick={() => setTab(id)}
                  onKeyDown={(event) => {
                    const next =
                      event.key === 'ArrowRight'
                        ? (index + 1) % tabs.length
                        : event.key === 'ArrowLeft'
                          ? (index + tabs.length - 1) % tabs.length
                          : event.key === 'Home'
                            ? 0
                            : event.key === 'End'
                              ? tabs.length - 1
                              : null
                    if (next === null) return
                    event.preventDefault()
                    const nextTab = tabs[next]!
                    setTab(nextTab)
                    document.getElementById(`settings-tab-${nextTab}`)?.focus()
                  }}
                  className={`min-h-11 px-2 py-3 text-[11px] tracking-[0.08em] border-b-2 transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524] focus-visible:-outline-offset-2 ${tab === id ? 'border-[#F5A524] text-[#F3E7D0] bg-[#18130D]' : 'border-transparent text-[#C7AE86] hover:text-[#F3E7D0] hover:bg-[#11100D]'}`}
                >
                  {t(`settings.tab.${id}`)}
                </button>
              ),
            )}
          </div>
          <div
            id={`settings-panel-${tab}`}
            role="tabpanel"
            aria-labelledby={`settings-tab-${tab}`}
            className="settings-panel min-h-0 flex-1 overflow-y-auto px-3 md:px-5 py-2"
          >
            {tab === 'launcher' && (
              <>
                <LauncherUpdatePanel />
                <DevPanel />
              </>
            )}
            {tab === 'general' && (
              <>
                <Setting
                  icon={<UserRound size={14} />}
                  label={t('settings.account')}
                  hint={t('settings.accountHint')}
                >
                  <div className="flex w-full min-w-0 flex-col gap-2">
                    <div className="flex w-full min-w-0 flex-wrap items-center gap-2">
                      <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-neutral-200">
                        {username}
                      </span>
                      <button
                        type="button"
                        aria-expanded={passwordOpen}
                        onClick={() => setPasswordOpen((open) => !open)}
                        className="h-10 px-3 flex items-center gap-2 border border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
                      >
                        <KeyRound size={12} />
                        {t('settings.changePassword')}
                      </button>
                      <button
                        type="button"
                        disabled={accountBusy}
                        onClick={() => void handleAccount('logout')}
                        className="h-10 px-3 flex items-center gap-2 border border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] disabled:opacity-50 transition-colors"
                      >
                        <LogOut size={12} />
                        {t('settings.logout')}
                      </button>
                    </div>
                    {passwordOpen && (
                      <form
                        className="flex w-full"
                        onSubmit={(e) => {
                          e.preventDefault()
                          void handleAccount('change_password')
                        }}
                      >
                        <input
                          type="password"
                          autoFocus
                          autoComplete="new-password"
                          aria-label={t('settings.newPassword')}
                          placeholder={t('settings.newPassword')}
                          value={newPassword}
                          onChange={(e) => setNewPassword(e.target.value)}
                          className="relative focus:z-10 min-w-0 flex-1 h-10 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 placeholder:text-[#A89373] outline-none focus:border-[#F5A524]/60 transition-colors"
                        />
                        <button
                          type="submit"
                          disabled={accountBusy || !newPassword}
                          className="relative -ml-px h-10 shrink-0 px-3 border border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] disabled:opacity-50 transition-colors"
                        >
                          {accountBusy ? (
                            <LoaderCircle size={12} className="animate-spin" />
                          ) : (
                            t('settings.save')
                          )}
                        </button>
                      </form>
                    )}
                    {accountError && (
                      <ErrorDetail message={accountError.message} raw={accountError.raw} />
                    )}
                    {passwordChanged && (
                      <span className="text-[11px] text-[#F5A524]">
                        {t('settings.passwordChanged')}
                      </span>
                    )}
                  </div>
                </Setting>
                <Setting
                  icon={<Folder size={14} />}
                  label={isWorkshop ? 'Папка мастерской' : t('settings.gameDir')}
                  hint={
                    isWorkshop
                      ? 'Настройки, клавиши, миры и логи мастерской хранятся отдельно от игры.'
                      : t('settings.gameDirHint')
                  }
                >
                  <PathInput
                    label={isWorkshop ? 'Папка мастерской' : t('settings.gameDir')}
                    value={dir}
                    onChange={(v) => {
                      setDir(v)
                      markDirty()
                    }}
                    browseLabel={t('settings.browse')}
                    onBrowse={() => handleBrowse('dir')}
                  />
                </Setting>
                {isWorkshop && (
                  <Setting
                    icon={<Gamepad2 size={14} />}
                    label="Адрес мастерской"
                    hint="Отдельный хост подключения к мастерской. Аккаунт должен иметь доступ к строительству."
                  >
                    <input
                      aria-label="Адрес мастерской"
                      value={workshopServer}
                      onChange={(event) => {
                        setWorkshopServer(event.target.value)
                        markDirty()
                      }}
                      placeholder="Хост или хост:порт"
                      className="h-10 w-full min-w-0 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] text-[#F3E7D0] placeholder:text-[#C7AE86] focus-visible:outline-2 focus-visible:outline-[#F5A524]"
                    />
                  </Setting>
                )}
                <Setting
                  compact
                  icon={<RefreshCw size={14} />}
                  label={t('settings.autoUpdate')}
                  hint={t('settings.autoUpdateHint')}
                >
                  <Toggle
                    label={t('settings.autoUpdate')}
                    on={autoUpdate}
                    onChange={(v) => {
                      setAutoUpdate(v)
                      markDirty()
                    }}
                    onLabel={t('settings.enabled')}
                    offLabel={t('settings.disabled')}
                  />
                </Setting>
                <Setting
                  compact
                  icon={<EyeOff size={14} />}
                  label="DISCORD RICH PRESENCE"
                  hint="Показывать в Discord состояние игры, карту и матч."
                >
                  <Toggle
                    label="Показывать активность в Discord"
                    on={discordPresence}
                    onChange={(value) => {
                      setDiscordPresence(value)
                      markDirty()
                    }}
                    onLabel={t('settings.enabled')}
                    offLabel={t('settings.disabled')}
                  />
                </Setting>
                <Setting
                  compact
                  icon={<EyeOff size={14} />}
                  label="СКРЫВАТЬ ВО ВРЕМЯ ИГРЫ"
                  hint="Окно вернётся после выхода. Открыть его раньше можно через ярлык лаунчера."
                >
                  <Toggle
                    label="Скрывать лаунчер во время игры"
                    on={hideWhilePlaying}
                    onChange={(value) => {
                      setHideWhilePlaying(value)
                      markDirty()
                    }}
                    onLabel={t('settings.enabled')}
                    offLabel={t('settings.disabled')}
                  />
                </Setting>
              </>
            )}
            {tab === 'runtime' && (
              <>
                <Setting
                  icon={<Coffee size={14} />}
                  label={t('settings.java')}
                  hint={t('settings.javaHint')}
                >
                  <PathInput
                    label={t('settings.java')}
                    value={java}
                    placeholder={t('settings.javaAuto')}
                    onChange={(v) => {
                      setJava(v)
                      markDirty()
                    }}
                    browseLabel={t('settings.browse')}
                    onBrowse={() => handleBrowse('java')}
                  />
                </Setting>
                <Setting
                  icon={<Cpu size={14} />}
                  label={t('settings.ram')}
                  hint={t('settings.ramHint', { gb: ram })}
                >
                  <RamSlider
                    ram={ram}
                    maxRam={maxRam}
                    onEdit={markDirty}
                    onChange={(v) => {
                      setRam(v)
                      markDirty()
                    }}
                  />
                </Setting>
              </>
            )}
            {tab === 'commands' && (
              <>
                <Setting
                  icon={<Terminal size={14} />}
                  label={t('settings.preLaunch')}
                  hint={t('settings.preLaunchHint')}
                >
                  <textarea
                    aria-label={t('settings.preLaunch')}
                    value={preLaunchCommand}
                    rows={3}
                    spellCheck={false}
                    placeholder={t('settings.commandPlaceholder')}
                    onChange={(e) => {
                      setPreLaunchCommand(e.target.value)
                      markDirty()
                    }}
                    className="command-input w-full min-w-0 resize-none border border-[#2A2116] bg-[#0B0906] p-3 text-[12px] font-mono text-neutral-200 focus-visible:outline-2 focus-visible:outline-[#F5A524]"
                  />
                </Setting>
                <Setting
                  icon={<Terminal size={14} />}
                  label={t('settings.postExit')}
                  hint={t('settings.postExitHint')}
                >
                  <textarea
                    aria-label={t('settings.postExit')}
                    value={postExitCommand}
                    rows={3}
                    spellCheck={false}
                    placeholder={t('settings.commandPlaceholder')}
                    onChange={(e) => {
                      setPostExitCommand(e.target.value)
                      markDirty()
                    }}
                    className="command-input w-full min-w-0 resize-none border border-[#2A2116] bg-[#0B0906] p-3 text-[12px] font-mono text-neutral-200 focus-visible:outline-2 focus-visible:outline-[#F5A524]"
                  />
                </Setting>
                <p className="pt-2 text-[11px] leading-relaxed text-[#C7AE86]">
                  {t('settings.commandsHint')}
                </p>
              </>
            )}
            {tab === 'skin' && (
              <div className="flex gap-6">
                <div className="min-w-0 flex-1">
                  <Setting
                    icon={<Shirt size={14} />}
                    label={t('settings.skin')}
                    hint={t('settings.skinHint')}
                  >
                    <PathInput
                      label={t('settings.skin')}
                      value={skinPath}
                      onChange={setSkinPath}
                      browseLabel={t('settings.browse')}
                      onBrowse={() => handleBrowsePng(setSkinPath)}
                    />
                  </Setting>
                  <Setting
                    icon={<Shirt size={14} />}
                    label={t('settings.cape')}
                    hint={t('settings.capeHint')}
                  >
                    <PathInput
                      label={t('settings.cape')}
                      value={capePath}
                      onChange={setCapePath}
                      browseLabel={t('settings.browse')}
                      onBrowse={() => handleBrowsePng(setCapePath)}
                    />
                  </Setting>
                  <Setting
                    compact
                    icon={<UserRound size={14} />}
                    label={t('settings.skinModel')}
                    hint={t('settings.skinModelHint')}
                  >
                    <Toggle
                      label={t('settings.skinModel')}
                      on={slim}
                      onChange={setSlim}
                      onLabel={t('settings.enabled')}
                      offLabel={t('settings.disabled')}
                    />
                  </Setting>
                  <div className="flex flex-wrap items-center justify-end gap-3 pt-3">
                    {skinMessage && skinMessage !== t('settings.skinUploaded') && (
                      <div className="max-w-[320px]">
                        <ErrorDetail message={skinMessage} raw={skinErrorRaw} />
                      </div>
                    )}
                    {skinMessage === t('settings.skinUploaded') && (
                      <span className="text-[11px] text-[#F5A524]">{skinMessage}</span>
                    )}
                    <button
                      type="button"
                      onClick={handleSkinUpload}
                      disabled={skinBusy || (!skinPath && !capePath)}
                      className="h-10 px-6 flex items-center gap-3 border border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      {skinBusy ? (
                        <LoaderCircle size={13} className="animate-spin text-[#F3E7D0]" />
                      ) : (
                        <Upload size={13} className="text-[#F3E7D0]" />
                      )}
                      <span className="text-[11px] tracking-[0.18em] text-[#F3E7D0]">
                        {skinBusy ? t('settings.skinUploading') : t('settings.skinUpload')}
                      </span>
                    </button>
                  </div>
                </div>
                <div className="shrink-0 self-start">
                  <SkinPreview
                    data={skinPreview}
                    caption={t(
                      skinPath
                        ? 'settings.skinPreview.local'
                        : skinPreview?.source === 'none'
                          ? 'settings.skinPreview.default'
                          : skinPreview?.source === 'mojang'
                            ? 'settings.skinPreview.mojang'
                            : 'settings.skinPreview.custom',
                    )}
                    fallback={t('settings.skinPreview.none')}
                  />
                </div>
              </div>
            )}
          </div>
          <div className="settings-actions shrink-0 px-3 md:px-5 py-3 flex flex-wrap items-center justify-end gap-2 border-t border-[#18130D] bg-[#0B0906]">
            <div className="flex flex-wrap items-center justify-end gap-3">
              <button
                type="button"
                onClick={handleReset}
                className="h-10 px-5 border border-[#2A2116] text-[#C7AE86] hover:text-neutral-200 hover:border-[#3A2C1D] transition-colors text-[11px] tracking-[0.18em]"
              >
                {t('settings.reset')}
              </button>
              <div className="flex flex-wrap items-center justify-end gap-2">
                {saveMessage && saveMessage !== t('settings.saved') && (
                  <div className="max-w-[320px]">
                    <ErrorDetail message={saveMessage} raw={saveErrorRaw} />
                  </div>
                )}
                <button
                  type="button"
                  onClick={handleSave}
                  disabled={!dirty || saving}
                  className={`h-10 px-6 flex items-center gap-3 border transition-colors ${
                    dirty
                      ? 'border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524]'
                      : 'border-[#2A2116] bg-[#0B0906] opacity-50 cursor-not-allowed'
                  }`}
                  style={
                    dirty
                      ? {
                          boxShadow:
                            '0 0 24px -8px rgba(245,165,36,0.4), inset 0 0 0 1px rgba(245,165,36,0.08)',
                        }
                      : undefined
                  }
                >
                  {saving ? (
                    <LoaderCircle size={13} className="animate-spin text-[#F3E7D0]" />
                  ) : (
                    <Save size={13} className="text-[#F3E7D0]" />
                  )}
                  <span className="text-[11px] tracking-[0.18em] text-[#F3E7D0]">
                    {saving
                      ? t('settings.saving')
                      : saveMessage === t('settings.saved')
                        ? 'СОХРАНЕНО'
                        : t('settings.save')}
                  </span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export function Setting({
  compact = false,
  icon,
  label,
  hint,
  children,
}: {
  compact?: boolean
  icon: ReactNode
  label: string
  hint: string
  children: ReactNode
}) {
  return (
    <div
      className={`grid ${compact ? 'grid-cols-[minmax(0,1fr)_64px]' : 'grid-cols-1'} md:grid-cols-[minmax(200px,260px)_minmax(0,1fr)] gap-3 md:gap-6 py-2 md:py-3 border-b border-dashed border-[#18130D] last:border-b-0`}
    >
      <div className="flex min-w-0 gap-3">
        <div className="mt-0.5 size-7 shrink-0 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#F5A524]">
          {icon}
        </div>
        <div className="flex min-w-0 flex-col">
          <span className="text-[11px] tracking-[0.12em] text-neutral-100">{label}</span>
          <span className="text-[11px] leading-snug text-[#8E7A5E] mt-1">{hint}</span>
        </div>
      </div>
      <div className="flex min-w-0 items-center">{children}</div>
    </div>
  )
}

export function PathInput({
  label,
  value,
  onChange,
  browseLabel,
  onBrowse,
  placeholder,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  browseLabel: string
  onBrowse: () => void
  placeholder?: string
}) {
  return (
    <div className="flex w-full">
      <input
        aria-label={label}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="relative focus:z-10 min-w-0 flex-1 h-10 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 placeholder:text-[#A89373] outline-none focus:border-[#F5A524]/60 transition-colors"
      />
      <button
        type="button"
        onClick={onBrowse}
        className="relative -ml-px h-10 shrink-0 px-3 border border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
      >
        {browseLabel}
      </button>
    </div>
  )
}

const RAM_MIN = 2
const RAM_STEP = 1
const RAM_PRESETS = [4, 6, 8, 12, 16, 24]

export function RamSlider({
  ram,
  maxRam,
  onChange,
  onEdit,
}: {
  ram: number
  maxRam: number
  onChange: (v: number) => void
  onEdit: () => void
}) {
  const clampRam = (value: number) => clampRamGb(value, maxRam)
  const pct = maxRam > RAM_MIN ? ((ram - RAM_MIN) / (maxRam - RAM_MIN)) * 100 : 0
  const set = (v: number) => onChange(clampRam(v))

  if (maxRam < RAM_MIN) {
    return <p role="alert">Недостаточно памяти: для игры нужно выделить минимум 2 ГБ.</p>
  }

  return (
    <div className="flex w-full min-w-0 flex-col gap-3">
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => set(ram - RAM_STEP)}
          className="size-9 shrink-0 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C] hover:text-[#F3E7D0] transition-colors"
          aria-label="Уменьшить память"
          disabled={ram <= RAM_MIN}
        >
          <Minus size={12} />
        </button>

        <div className="relative flex-1 min-w-8 h-1.5 bg-[#0B0906] border border-[#2A2116]">
          <div
            className="absolute top-0 left-0 h-full bg-gradient-to-r from-[#8A571C] to-[#F5A524]"
            style={{ width: `${pct}%` }}
          />
          <div
            className="absolute top-1/2 -translate-y-1/2 size-3 bg-[#F5A524] border border-[#070604] pointer-events-none"
            style={{
              left: `calc(${pct}% - 6px)`,
              boxShadow: '0 0 8px rgba(245,165,36,0.6)',
            }}
          />
          <input
            type="range"
            aria-label="Выделенная память, ГБ"
            min={RAM_MIN}
            max={maxRam}
            step={RAM_STEP}
            value={ram}
            onChange={(e) => set(Number(e.target.value))}
            className="absolute inset-x-0 top-1/2 -translate-y-1/2 h-9 w-full opacity-0 cursor-pointer"
          />
        </div>

        <button
          type="button"
          onClick={() => set(ram + RAM_STEP)}
          className="size-9 shrink-0 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C] hover:text-[#F3E7D0] transition-colors"
          aria-label="Увеличить память"
          disabled={ram >= maxRam}
        >
          <Plus size={12} />
        </button>

        <div className="shrink-0 flex items-baseline gap-1 w-20 justify-end">
          <input
            type="number"
            aria-label="Память, ГБ"
            min={RAM_MIN}
            max={maxRam}
            key={ram}
            defaultValue={ram}
            onChange={onEdit}
            onBlur={(e) => {
              const next = e.target.value.trim() ? clampRam(Number(e.target.value)) : ram
              e.target.value = String(next)
              if (next !== ram) set(next)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') e.currentTarget.blur()
            }}
            className="ram-number w-12 h-9 border border-[#2A2116] bg-[#0B0906] px-2 text-[12px] font-mono text-[#F3E7D0] text-right outline-none focus:border-[#F5A524]/60 transition-colors"
          />
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">GB</span>
        </div>
      </div>

      <div className="flex min-w-0 items-center gap-1" role="group" aria-label="Пресеты памяти">
        {RAM_PRESETS.filter((p) => p <= maxRam).map((p) => {
          const active = ram === p
          return (
            <button
              type="button"
              key={p}
              onClick={() => set(p)}
              aria-pressed={active}
              className={`h-7 min-w-0 flex-1 px-1 text-[10px] tracking-[0.14em] font-mono border transition-colors ${
                active
                  ? 'border-[#F5A524] bg-[#F5A524] text-[#070604]'
                  : 'border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C]'
              }`}
            >
              {p}G
            </button>
          )
        })}
      </div>
      <span className="text-right text-[10px] text-[#A89373]">
        Доступно для игры: {RAM_MIN}–{maxRam} ГБ
      </span>
    </div>
  )
}

function Toggle({
  label,
  on,
  onChange,
  onLabel,
  offLabel,
}: {
  label: string
  on: boolean
  onChange: (v: boolean) => void
  onLabel: string
  offLabel: string
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-label={label}
      aria-checked={on}
      onClick={() => onChange(!on)}
      className={`relative h-7 w-14 border transition-colors ${
        on ? 'border-[#F5A524]/60 bg-[#2A2116]' : 'border-[#2A2116] bg-[#0B0906]'
      }`}
    >
      <span
        className={`absolute top-1/2 -translate-y-1/2 size-5 transition-all ${
          on
            ? 'left-[30px] bg-[#F5A524] shadow-[0_0_10px_rgba(245,165,36,0.6)]'
            : 'left-0.5 bg-[#3A2C1D]'
        }`}
      />
      <span
        className={`absolute -bottom-5 right-0 text-[9px] tracking-[0.16em] ${
          on ? 'text-[#F5A524]' : 'text-[#8E7A5E]'
        }`}
      >
        {on ? onLabel : offLabel}
      </span>
    </button>
  )
}
