import { Download, RefreshCw, RotateCw, LoaderCircle } from 'lucide-react'
import {
  checkLauncherUpdate,
  installLauncherUpdate,
  restartLauncher,
  launcherUpdateBusy,
  launcherUpdateStatus,
  useLauncherUpdate,
} from '../../lib/launcher-update'

function bytes(value: number) {
  return value < 1024 * 1024
    ? `${Math.round(value / 1024)} КБ`
    : `${(value / (1024 * 1024)).toFixed(1)} МБ`
}

export function LauncherUpdatePanel() {
  const state = useLauncherUpdate()
  const busy = launcherUpdateBusy(state.phase)
  const transferring = ['downloading', 'verifying', 'installing'].includes(state.phase)
  const percent = state.total
    ? Math.min(100, Math.round((state.downloaded / state.total) * 100))
    : undefined
  const buttonClass =
    'min-h-10 px-4 inline-flex items-center justify-center gap-2 border border-[#8A571C] text-[12px] text-[#F3E7D0] hover:border-[#F5A524] disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-[#F5A524]'
  return (
    <section aria-label="Обновление лаунчера" className="h-full min-h-0 flex flex-col gap-4 py-3">
      <div className="shrink-0 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-[18px] text-[#F3E7D0]">Обновление лаунчера</h2>
        <span className="text-[12px] text-[#C7AE86]">Текущая версия: {state.currentVersion}</span>
      </div>
      <div className="shrink-0 space-y-2">
        <p
          role="status"
          aria-live="polite"
          className={`text-[13px] ${state.phase === 'error' ? 'text-[#c98b8b]' : state.phase === 'current' || state.installed ? 'text-[#82D66B]' : 'text-[#F3E7D0]'}`}
        >
          {launcherUpdateStatus(state)}
        </p>
        {transferring && (
          <>
            <progress
              aria-label="Загрузка обновления лаунчера"
              max={100}
              value={state.phase === 'downloading' ? percent : 100}
              className="launcher-update-progress block w-full h-2"
            />
            <div className="flex justify-between gap-2 text-[12px] text-[#C7AE86] tabular-nums">
              <span>
                {bytes(state.downloaded)}
                {state.total ? ` / ${bytes(state.total)}` : ' · размер не указан сервером'}
              </span>
              {percent !== undefined && <span>{percent}%</span>}
            </div>
          </>
        )}
        {state.checkedAt && (
          <p className="text-[11px] text-[#C7AE86]">
            Последняя проверка: {new Date(state.checkedAt).toLocaleString('ru-RU')}
          </p>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain text-[12px] leading-relaxed text-[#C7AE86]">
        {state.error ? (
          <p role="alert" className="break-words text-[#c98b8b]">
            {state.error}
          </p>
        ) : state.update?.body ? (
          <p className="whitespace-pre-wrap break-words">{state.update.body}</p>
        ) : (
          <p>
            Новые версии проверяются при запуске лаунчера. Установка начинается по вашей команде.
          </p>
        )}
      </div>
      <div className="shrink-0 flex flex-wrap gap-3">
        {!state.installed && (
          <button
            onClick={() => void checkLauncherUpdate()}
            disabled={busy}
            className={buttonClass}
          >
            {state.phase === 'checking' ? (
              <LoaderCircle size={14} className="animate-spin" />
            ) : (
              <RefreshCw size={14} />
            )}
            Проверить обновления
          </button>
        )}
        {state.update && !state.installed && (
          <button
            onClick={() => void installLauncherUpdate()}
            disabled={busy}
            className={`${buttonClass} bg-[#2A2116]`}
          >
            <Download size={14} />{' '}
            {busy && state.phase !== 'checking'
              ? 'Обновление выполняется'
              : `Установить ${state.update.version}`}
          </button>
        )}
        {state.installed && (
          <button
            onClick={() => void restartLauncher()}
            disabled={busy}
            className={`${buttonClass} bg-[#2A2116]`}
          >
            <RotateCw size={14} /> Перезапустить лаунчер
          </button>
        )}
      </div>
    </section>
  )
}
