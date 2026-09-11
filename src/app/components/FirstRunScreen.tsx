import { useState } from 'react'
import { Folder, Coffee, Cpu, UserRound, LoaderCircle, ArrowRight } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { ErrorDetail } from './ui-bits'
import { Setting, PathInput, RamSlider } from './SettingsScreen'
import type { LauncherConfig } from '../../lib/api'
import { friendlyError } from '../../lib/errors'
import { isValidUsername } from '../../lib/username'

/**
 * Shown once, before the first launch — replaces the old behavior of silently
 * redirecting to Settings (whose default tab is whichever one was last opened).
 * `defaults` come from `load_settings`, which already returns sane platform defaults
 * (game dir, bundled Java, 4 GiB RAM) even when nothing has been saved yet.
 */
export function FirstRunScreen({
  defaults,
  onComplete,
}: {
  defaults: LauncherConfig
  onComplete: (config: LauncherConfig) => void
}) {
  const [name, setName] = useState('')
  const [dir, setDir] = useState(defaults.gameDir)
  const [java, setJava] = useState(defaults.javaPath)
  const [ram, setRam] = useState(Math.max(2, Math.round(defaults.ramMb / 1024)))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<{ message: string; raw: string | null } | null>(null)
  const touched = name.length > 0
  const nameValid = isValidUsername(name)

  const handleBrowse = async (field: 'dir' | 'java') => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({
        directory: field === 'dir',
        title: field === 'dir' ? 'Select Game Directory' : 'Select Java Executable',
      })
      if (selected) {
        if (field === 'dir') setDir(selected as string)
        else setJava(selected as string)
      }
    } catch (e) {
      setError({
        message: 'Не удалось открыть выбор файла. Укажите путь вручную или повторите.',
        raw: String(e),
      })
    }
  }

  const handleContinue = async () => {
    if (!nameValid || saving) return
    setSaving(true)
    setError(null)
    try {
      const config: LauncherConfig = {
        ...defaults,
        gameDir: dir,
        javaPath: java,
        ramMb: ram * 1024,
        username: name.trim(),
      }
      await invoke('save_settings', { config })
      onComplete(config)
    } catch (e) {
      console.error('Failed to save initial settings:', e)
      setError(friendlyError(e, 'settings-save'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="relative h-full w-full overflow-y-auto bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.4} />

      <div className="screen-layout relative min-h-full flex flex-col items-center justify-center gap-4 px-4 py-6">
        <div className="w-full max-w-[640px] flex flex-col gap-4">
          <div className="shrink-0">
            <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
              ПЕРВЫЙ ЗАПУСК
            </h1>
            <p className="mt-2 text-[11px] tracking-[0.1em] text-[#8E7A5E]">
              Задайте позывной и проверьте параметры игры. Остальное можно изменить позже в
              настройках.
            </p>
          </div>

          <div className="border border-[#2A2116] bg-[#11100D] px-3 md:px-5 py-2">
            <Setting
              icon={<UserRound size={14} />}
              label="НИКНЕЙМ"
              hint="Ник в Minecraft (3–16 латинских букв, цифр или _). Обязателен для входа в игру."
            >
              <input
                aria-label="Никнейм"
                aria-invalid={touched && !nameValid}
                autoFocus
                value={name}
                maxLength={16}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleContinue()
                }}
                className={`w-full h-10 border bg-[#0B0906] px-3 text-[12px] font-mono text-neutral-200 outline-none transition-colors ${
                  touched && !nameValid
                    ? 'border-[#c98b8b] focus:border-[#c98b8b]'
                    : 'border-[#2A2116] focus:border-[#F5A524]/60'
                }`}
              />
            </Setting>
            <Setting
              icon={<Folder size={14} />}
              label="ПАПКА ИГРЫ"
              hint="Рекомендуемое значение по умолчанию — можно изменить."
            >
              <PathInput
                label="Папка игры"
                value={dir}
                onChange={setDir}
                browseLabel="ОБЗОР"
                onBrowse={() => void handleBrowse('dir')}
              />
            </Setting>
            <Setting
              icon={<Coffee size={14} />}
              label="СРЕДА JAVA"
              hint="Рекомендуемое значение по умолчанию — можно изменить."
            >
              <PathInput
                label="Java"
                value={java}
                onChange={setJava}
                browseLabel="ОБЗОР"
                onBrowse={() => void handleBrowse('java')}
              />
            </Setting>
            <Setting
              icon={<Cpu size={14} />}
              label="ВЫДЕЛЕНИЕ RAM"
              hint={`${ram} ГБ выделено · по умолчанию 4 ГБ`}
            >
              <RamSlider ram={ram} onChange={setRam} onEdit={() => {}} />
            </Setting>
          </div>

          {error && <ErrorDetail message={error.message} raw={error.raw} />}

          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => void handleContinue()}
              disabled={!nameValid || saving}
              className={`h-10 px-6 flex items-center gap-3 border transition-colors ${
                nameValid
                  ? 'border-[#F5A524]/40 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524]'
                  : 'border-[#2A2116] bg-[#0B0906] opacity-50 cursor-not-allowed'
              }`}
            >
              {saving ? (
                <LoaderCircle size={13} className="animate-spin text-[#F3E7D0]" />
              ) : (
                <ArrowRight size={13} className="text-[#F3E7D0]" />
              )}
              <span className="text-[11px] tracking-[0.18em] text-[#F3E7D0]">ПРОДОЛЖИТЬ</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
