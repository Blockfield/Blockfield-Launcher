import { ramLimitGb, clampRamGb } from '../../lib/ram'
import { useState } from 'react'
import { Folder, Coffee, Cpu, LoaderCircle, ArrowRight } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { ErrorDetail } from './ui-bits'
import { Setting, PathInput, RamSlider } from './SettingsScreen'
import type { LauncherConfig } from '../../lib/api'
import { friendlyError } from '../../lib/errors'
import { selectedGameDirectory } from '../../lib/game-directory'
import { translate } from '../i18n'

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
  const [dir, setDir] = useState(defaults.gameDir)
  const [java, setJava] = useState(defaults.javaPath)
  const maxRam = ramLimitGb(defaults.maxRamMb)
  const [ram, setRam] = useState(clampRamGb(defaults.ramMb / 1024, maxRam))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<{ message: string; raw: string | null } | null>(null)

  const handleBrowse = async (field: 'dir' | 'java') => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({
        directory: field === 'dir',
        title: field === 'dir' ? 'Select Game Directory' : 'Select Java Executable',
      })
      if (selected) {
        if (field === 'dir') setDir(await selectedGameDirectory(selected as string))
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
    if (saving || maxRam < 2) return
    setSaving(true)
    setError(null)
    try {
      const config: LauncherConfig = {
        ...defaults,
        gameDir: dir,
        javaPath: java,
        ramMb: ram * 1024,
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
              Проверьте параметры игры. Их можно изменить позже в настройках.
            </p>
          </div>

          <div className="border border-[#2A2116] bg-[#11100D] px-3 md:px-5 py-2">
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
              hint={translate('settings.javaHint')}
            >
              <PathInput
                label="Java"
                value={java}
                placeholder={translate('settings.javaAuto')}
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
              <RamSlider maxRam={maxRam} ram={ram} onChange={setRam} onEdit={() => {}} />
            </Setting>
          </div>

          {error && <ErrorDetail message={error.message} raw={error.raw} />}

          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => void handleContinue()}
              disabled={saving || maxRam < 2}
              className={`h-10 px-6 flex items-center gap-3 border transition-colors ${
                maxRam >= 2
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
