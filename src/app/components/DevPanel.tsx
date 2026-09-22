import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { FlaskConical } from 'lucide-react'
import { Setting } from './SettingsScreen'

/** What a dev build does differently; `null` in a production build, where this panel is invisible. */
export interface DevStatus {
  server: string
  freezePack: boolean
  jvmArgs: string
  packUrl: string
}

/**
 * Dev builds are configured by environment variables, not by the UI: the panel only shows what is
 * in effect, so a developer can tell at a glance where the game will connect and whether packwiz
 * will overwrite their jars. Labels stay untranslated on purpose — players never see this.
 */
export function DevPanel() {
  const [status, setStatus] = useState<DevStatus | null>(null)

  useEffect(() => {
    invoke<DevStatus | null>('dev_status')
      .then(setStatus)
      .catch(() => setStatus(null))
  }, [])

  if (!status) return null
  const rows: [string, string][] = [
    ['BLOCKFIELD_DEV_SERVER', status.server || 'сервер пака'],
    ['BLOCKFIELD_DEV_FREEZE_PACK', status.freezePack ? 'пак заморожен' : 'выкл'],
    ['BLOCKFIELD_DEV_JVM_ARGS', status.jvmArgs || '—'],
    ['VITE_BLOCKFIELD_PACK_URL', status.packUrl],
  ]

  return (
    <Setting
      icon={<FlaskConical size={14} />}
      label="DEV-сборка"
      hint="Настройки берутся из переменных окружения при запуске лаунчера"
    >
      <div className="flex w-full min-w-0 flex-col gap-1 font-mono text-[11px] text-[#C7AE86]">
        {rows.map(([name, value]) => (
          <div key={name} className="flex min-w-0 gap-2">
            <span className="shrink-0 text-[#8E7A5E]">{name}</span>
            <span className="min-w-0 truncate text-neutral-200">{value}</span>
          </div>
        ))}
      </div>
    </Setting>
  )
}
