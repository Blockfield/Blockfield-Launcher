import { useCallback, useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { Folder, Cpu, Coffee, Globe, RefreshCw, LogOut, Save, Minus, Plus, LoaderCircle } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { GlowPanel } from './ui-bits'
import { LANGUAGES, useI18n, type Lang } from '../i18n'
import { OPERATOR_HANDLE } from '../constants'
import type { LauncherConfig } from '../../lib/api'

/** Detect whether we're running inside Tauri. */
const isTauri = () => '__TAURI_INTERNALS__' in window

export function SettingsScreen({ onLogout }: { onLogout: () => void }) {
  const { lang, setLang, t } = useI18n()
  const [dir, setDir] = useState('')
  const [java, setJava] = useState('')
  const [ram, setRam] = useState(8)
  const [autoUpdate, setAutoUpdate] = useState(true)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saveMessage, setSaveMessage] = useState<string | null>(null)

  // Load settings on mount
  useEffect(() => {
    if (!isTauri()) {
      queueMicrotask(() => setLoading(false))
      return
    }

    invoke<LauncherConfig>('load_settings')
      .then((cfg) => {
        setDir(cfg.gameDir)
        setJava(cfg.javaPath)
        setRam(Math.max(2, Math.round(cfg.ramMb / 1024)))
        setAutoUpdate(cfg.autoUpdate)
        if (cfg.lang !== lang && (cfg.lang === 'en' || cfg.lang === 'ru' || cfg.lang === 'uk')) {
          setLang(cfg.lang as Lang)
        }
      })
      .catch((e) => console.error('Failed to load settings:', e))
      .finally(() => setLoading(false))
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const markDirty = useCallback(() => setDirty(true), [])

  const handleSave = useCallback(async () => {
    if (!isTauri()) return
    setSaving(true)
    setSaveMessage(null)
    try {
      await invoke('save_settings', {
        config: {
          gameDir: dir,
          javaPath: java,
          ramMb: ram * 1024,
          autoUpdate,
          lang,
        } satisfies LauncherConfig,
      })
      setDirty(false)
      setSaveMessage(t('settings.saved'))
      setTimeout(() => setSaveMessage(null), 3000)
    } catch (e) {
      console.error('Failed to save settings:', e)
      setSaveMessage(String(e))
    } finally {
      setSaving(false)
    }
  }, [dir, java, ram, autoUpdate, lang, t])

  const handleReset = useCallback(() => {
    invoke<LauncherConfig>('load_settings')
      .then((cfg) => {
        setDir(cfg.gameDir)
        setJava(cfg.javaPath)
        setRam(Math.max(2, Math.round(cfg.ramMb / 1024)))
        setAutoUpdate(cfg.autoUpdate)
      })
      .catch(console.error)
    setDirty(false)
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
          if (field === 'dir') setDir(path)
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

      <div className="relative h-full p-6 flex flex-col overflow-y-auto">
        <div className="flex items-center gap-3 mb-5">
          <span className="shrink-0 text-[10px] tracking-[0.22em] text-[#8E7A5E]">
            {t('settings.configuration')}
          </span>
          <span className="h-px flex-1 bg-[#18130D]" />
          <span className="shrink-0 text-[10px] tracking-[0.22em] text-[#8E7A5E]">
            {t('settings.operator', { handle: OPERATOR_HANDLE })}
          </span>
        </div>

        <div className="flex items-baseline gap-3">
          <h1 className="tracking-[0.06em] text-[34px] leading-none text-neutral-50">
            {t('nav.settings')}
          </h1>
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
            {t('settings.preferences')}
          </span>
        </div>

        <GlowPanel glow={false} className="mt-5">
          <Group title={t('settings.runtime')} code="ENV-001">
            <Setting
              icon={<Folder size={14} />}
              label={t('settings.gameDir')}
              hint={t('settings.gameDirHint')}
            >
              <PathInput
                value={dir}
                onChange={(v) => {
                  setDir(v)
                  markDirty()
                }}
                browseLabel={t('settings.browse')}
                onBrowse={() => handleBrowse('dir')}
              />
            </Setting>
            <Setting
              icon={<Coffee size={14} />}
              label={t('settings.java')}
              hint={t('settings.javaHint')}
            >
              <PathInput
                value={java}
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
                onChange={(v) => {
                  setRam(v)
                  markDirty()
                }}
              />
            </Setting>
          </Group>

          <Group title={t('settings.launcher')} code="LCH-002">
            <Setting
              icon={<Globe size={14} />}
              label={t('settings.language')}
              hint={t('settings.languageHint')}
            >
              <div className="flex flex-wrap gap-1">
                {LANGUAGES.map((l) => (
                  <button
                    type="button"
                    key={l.code}
                    aria-pressed={lang === l.code}
                    onClick={() => {
                      setLang(l.code)
                      markDirty()
                    }}
                    className={`h-9 px-3 text-[10px] tracking-[0.14em] border transition-colors ${
                      lang === l.code
                        ? 'border-[#F5A524] bg-[#2A2116] text-[#F3E7D0]'
                        : 'border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C]'
                    }`}
                  >
                    {l.label}
                  </button>
                ))}
              </div>
            </Setting>

            <Setting
              icon={<RefreshCw size={14} />}
              label={t('settings.autoUpdate')}
              hint={t('settings.autoUpdateHint')}
            >
              <Toggle
                on={autoUpdate}
                onChange={(v) => {
                  setAutoUpdate(v)
                  markDirty()
                }}
                onLabel={t('settings.enabled')}
                offLabel={t('settings.disabled')}
              />
            </Setting>
          </Group>

          <div className="px-6 py-4 flex items-center justify-between border-t border-[#18130D] bg-[#0B0906]">
            <button
              type="button"
              onClick={onLogout}
              className="h-10 px-5 flex items-center gap-3 border border-[#3a2828] bg-[#1a0e0e] text-[#c98b8b] hover:border-[#7a3838] hover:text-[#e0a3a3] transition-colors"
            >
              <LogOut size={13} />
              <span className="text-[11px] tracking-[0.18em]">{t('settings.logout')}</span>
            </button>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={handleReset}
                className="h-10 px-5 border border-[#2A2116] text-[#C7AE86] hover:text-neutral-200 hover:border-[#3A2C1D] transition-colors text-[11px] tracking-[0.18em]"
              >
                {t('settings.reset')}
              </button>
              <div className="flex items-center gap-2">
                {saveMessage && (
                  <span
                    className={`text-[9px] tracking-[0.16em] ${
                      saveMessage === t('settings.saved')
                        ? 'text-[#8E7A5E]'
                        : 'text-[#c98b8b]'
                    }`}
                  >
                    {saveMessage}
                  </span>
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
                    {saving ? t('settings.saving') : t('settings.save')}
                  </span>
                </button>
              </div>
            </div>
          </div>
        </GlowPanel>
      </div>
    </div>
  )
}

function Group({ title, code, children }: { title: string; code: string; children: ReactNode }) {
  return (
    <div className="border-b border-[#18130D] last:border-b-0">
      <div className="px-6 pt-4 pb-2 flex items-center gap-3">
        <span className="size-1.5 bg-[#F5A524]" />
        <span className="text-[10px] tracking-[0.18em] text-[#F3E7D0]">{title}</span>
        <span className="h-px flex-1 bg-[#18130D]" />
        <span className="text-[9px] tracking-[0.28em] text-[#5E5040]">{code}</span>
      </div>
      <div className="px-6 pb-1">{children}</div>
    </div>
  )
}

function Setting({
  icon,
  label,
  hint,
  children,
}: {
  icon: ReactNode
  label: string
  hint: string
  children: ReactNode
}) {
  return (
    <div className="grid grid-cols-[minmax(230px,280px)_minmax(0,1fr)] gap-6 py-3 border-b border-dashed border-[#18130D] last:border-b-0">
      <div className="flex min-w-0 gap-3">
        <div className="mt-0.5 size-7 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#F5A524]">
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

function PathInput({
  value,
  onChange,
  browseLabel,
  onBrowse,
}: {
  value: string
  onChange: (v: string) => void
  browseLabel: string
  onBrowse: () => void
}) {
  return (
    <div className="flex w-full">
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 h-10 border border-[#2A2116] bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 outline-none focus:border-[#F5A524]/60 transition-colors"
      />
      <button
        type="button"
        onClick={onBrowse}
        className="h-10 shrink-0 px-3 border border-l-0 border-[#2A2116] bg-[#11100D] text-[10px] tracking-[0.14em] text-[#C7AE86] hover:text-[#F3E7D0] hover:border-[#8A571C] transition-colors"
      >
        {browseLabel}
      </button>
    </div>
  )
}

const RAM_MIN = 2
const RAM_MAX = 32
const RAM_STEP = 1
const RAM_PRESETS = [4, 6, 8, 12, 16, 24]

function clampRam(v: number) {
  if (Number.isNaN(v)) return RAM_MIN
  return Math.min(RAM_MAX, Math.max(RAM_MIN, Math.round(v)))
}

function RamSlider({ ram, onChange }: { ram: number; onChange: (v: number) => void }) {
  const pct = ((ram - RAM_MIN) / (RAM_MAX - RAM_MIN)) * 100
  const set = (v: number) => onChange(clampRam(v))

  return (
    <div className="flex w-full min-w-0 flex-col gap-3">
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => set(ram - RAM_STEP)}
          className="size-9 shrink-0 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C] hover:text-[#F3E7D0] transition-colors"
          aria-label="decrease"
        >
          <Minus size={12} />
        </button>

        <div className="relative flex-1 h-1.5 bg-[#0B0906] border border-[#2A2116]">
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
            min={RAM_MIN}
            max={RAM_MAX}
            step={RAM_STEP}
            value={ram}
            onChange={(e) => set(parseInt(e.target.value))}
            className="absolute inset-0 w-full opacity-0 cursor-pointer"
          />
        </div>

        <button
          type="button"
          onClick={() => set(ram + RAM_STEP)}
          className="size-9 shrink-0 grid place-items-center border border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C] hover:text-[#F3E7D0] transition-colors"
          aria-label="increase"
        >
          <Plus size={12} />
        </button>

        <div className="shrink-0 flex items-baseline gap-1 w-20 justify-end">
          <input
            type="number"
            min={RAM_MIN}
            max={RAM_MAX}
            value={ram}
            onChange={(e) => set(parseInt(e.target.value))}
            className="w-12 h-9 border border-[#2A2116] bg-[#0B0906] px-2 text-[12px] font-mono text-[#F3E7D0] text-right outline-none focus:border-[#F5A524]/60 transition-colors"
          />
          <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">GB</span>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-[9px] tracking-[0.22em] text-[#5E5040] mr-1">PRESET</span>
        {RAM_PRESETS.map((p) => {
          const active = ram === p
          return (
            <button
              type="button"
              key={p}
              onClick={() => set(p)}
              aria-pressed={active}
              className={`h-7 px-2.5 text-[10px] tracking-[0.14em] font-mono border transition-colors ${
                active
                  ? 'border-[#F5A524] bg-[#2A2116] text-[#F3E7D0]'
                  : 'border-[#2A2116] bg-[#0B0906] text-[#C7AE86] hover:border-[#8A571C]'
              }`}
            >
              {p}G
            </button>
          )
        })}
        <span className="ml-auto text-[9px] tracking-[0.22em] text-[#5E5040] font-mono">
          {RAM_MIN}–{RAM_MAX} GB
        </span>
      </div>
    </div>
  )
}

function Toggle({
  on,
  onChange,
  onLabel,
  offLabel,
}: {
  on: boolean
  onChange: (v: boolean) => void
  onLabel: string
  offLabel: string
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!on)}
      className={`relative h-7 w-14 border transition-colors ${
        on ? 'border-[#F5A524]/60 bg-[#2A2116]' : 'border-[#2A2116] bg-[#0B0906]'
      }`}
    >
      <span
        className={`absolute top-0.5 size-5 transition-all ${
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
