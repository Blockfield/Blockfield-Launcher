import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Play, Pause, Download, HardDrive, FileBox, Zap, LoaderCircle } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { GlowPanel, OperationBar, SectionHeader, StatusDot } from './ui-bits'
import { useI18n, type TKey, type TFunction } from '../i18n'
import { OPERATION_NAME } from '../constants'
import { listenDownloadProgress } from '../../lib/events'
import type { DownloadProgress, VersionCheckResult } from '../../lib/api'

type StepStatus = 'done' | 'active' | 'pending'
type Phase = 'checking' | 'downloading' | 'verifying' | 'complete' | 'error' | 'uptodate'

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

/** Format bytes per second into a human-readable string. */
function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec} B/s`
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`
}

/** Format bytes into human-readable string. */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

/** Generate a timestamp string for the operation log. */
function timestamp(): string {
  const now = new Date()
  return now.toTimeString().slice(0, 8)
}

type LogEntry = { ts: string; tone: 'ok' | 'info' | 'dim' | 'warn'; msg: string }

export function UpdateScreen() {
  const { t } = useI18n()
  const [phase, setPhase] = useState<Phase>('checking')
  const [progress, setProgress] = useState(0)
  const [currentFile, setCurrentFile] = useState('')
  const [fileIdx, setFileIdx] = useState(0)
  const [fileCount, setFileCount] = useState(0)
  const [downloadedBytes, setDownloadedBytes] = useState(0)
  const [totalBytes, setTotalBytes] = useState(0)
  const [speed, setSpeed] = useState('—')
  const [logLines, setLogLines] = useState<LogEntry[]>([])
  const [error, setError] = useState<string | null>(null)
  const [verifyFailed, setVerifyFailed] = useState<string[]>([])
  const [manifestVersion, setManifestVersion] = useState('')
  const [steps, setSteps] = useState<Array<{ label: TKey; status: StepStatus }>>([
    { label: 'update.step.verify', status: 'pending' },
    { label: 'update.step.prune', status: 'pending' },
    { label: 'update.step.download', status: 'pending' },
    { label: 'update.step.integrity', status: 'pending' },
    { label: 'update.step.finalize', status: 'pending' },
  ])

  const unlistenRef = useRef<UnlistenFn | null>(null)
  const cancelledRef = useRef(false)
  const startedRef = useRef(false)

  const addLog = useCallback((msg: string, tone: LogEntry['tone'] = 'info') => {
    setLogLines((prev) => [...prev.slice(-49), { ts: timestamp(), tone, msg }])
  }, [])

  const setStep = useCallback(
    (index: number, status: StepStatus) => {
      setSteps((prev) => prev.map((s, i) => (i === index ? { ...s, status } : s)))
    },
    [],
  )

  // Start the update flow on mount
  useEffect(() => {
    if (!isTauri()) {
      queueMicrotask(() => {
        setPhase('error')
        setError('Update flow requires the Tauri desktop environment.')
        addLog('ERROR: Not running inside Tauri — update unavailable', 'warn')
      })
      return
    }

    // Guard against React StrictMode double-mount in dev
    if (startedRef.current) return
    startedRef.current = true

    const run = async () => {
      cancelledRef.current = false
      setPhase('checking')
      addLog('Establishing connection to deployment server...', 'info')

      // Step 1: Check for updates
      setStep(0, 'active')
      let versionResult: VersionCheckResult
      try {
        versionResult = await invoke<VersionCheckResult>('check_modpack_version')
        addLog(
          `Manifest fetched: remote=${versionResult.remoteVersion}, installed=${versionResult.installedVersion}`,
          'ok',
        )
      } catch (e) {
        addLog(`ERROR: Failed to fetch manifest — ${String(e)}`, 'warn')
        setPhase('error')
        setError(String(e))
        return
      }

      setStep(0, 'done')

      const needJava = versionResult.java && !versionResult.javaOk
      const needForge = !versionResult.forgeOk
      const needModpack = versionResult.needsUpdate
      const needAnyDownload = needModpack || needJava || needForge

      if (needForge) {
        addLog(`Forge ${versionResult.java?.version ?? ''} required — will install`, 'info')
      }
      if (needJava) {
        addLog(`Java ${versionResult.java!.version} required — will install`, 'info')
      }

      if (!needAnyDownload) {
        addLog('Modpack is up to date — no download needed.', 'ok')
        setManifestVersion(versionResult.remoteVersion)
        setProgress(100)
        setFileCount(versionResult.fileCount)
        setTotalBytes(versionResult.totalSize)
        setDownloadedBytes(versionResult.totalSize)
        setPhase('uptodate')
        setStep(0, 'done')
        setStep(1, 'done')
        setStep(2, 'done')
        setStep(3, 'done')
        setStep(4, 'done')
        return
      }

      setManifestVersion(versionResult.remoteVersion)
      setFileCount(versionResult.fileCount)
      setTotalBytes(versionResult.totalSize)

      // Step 2: Prune stale files
      setStep(1, 'active')
      addLog(`Update needed: ${versionResult.installedVersion} → ${versionResult.remoteVersion}`, 'info')
      addLog(`Pruning stale assets... (${versionResult.fileCount} files queued)`, 'info')
      setStep(1, 'done')

      // Step 3: Download
      setStep(2, 'active')
      addLog('Phase 1/2 — downloading modpack payload', 'info')
      setPhase('downloading')

      // Listen for progress events
      const unlisten = await listenDownloadProgress((p: DownloadProgress) => {
        if (p.totalBytesAll > 0) {
          const pct = (p.totalBytesDownloaded / p.totalBytesAll) * 100
          setProgress(Math.min(100, pct))
        }
        setFileIdx(p.fileIndex)
        if (p.filePath) setCurrentFile(p.filePath)
        setDownloadedBytes(p.totalBytesDownloaded)
        if (p.speedBytesPerSec > 0) setSpeed(formatSpeed(p.speedBytesPerSec))
      })
      unlistenRef.current = unlisten

      try {
        await invoke('download_modpack')
        addLog(`Download complete: ${formatBytes(versionResult.totalSize)} transferred`, 'ok')
        setProgress(100)
        setStep(2, 'done')
      } catch (e) {
        if (cancelledRef.current) {
          addLog('Download cancelled by operator.', 'dim')
          setPhase('error')
          setError('Download cancelled')
        } else {
          addLog(`ERROR: Download failed — ${String(e)}`, 'warn')
          setPhase('error')
          setError(String(e))
        }
        return
      } finally {
        unlistenRef.current?.()
        unlistenRef.current = null
      }

      // Step 4: Verify integrity
      setStep(3, 'active')
      setPhase('verifying')
      addLog('Verifying file integrity (SHA-256)...', 'info')

      try {
        const failed = await invoke<string[]>('verify_files')
        setVerifyFailed(failed)
        if (failed.length > 0) {
          addLog(`WARNING: ${failed.length} file(s) failed integrity check`, 'warn')
          failed.slice(0, 10).forEach((f) => addLog(`  ↳ ${f}`, 'dim'))
          setPhase('error')
          setError(`${failed.length} file(s) failed verification`)
          return
        }
        addLog(`All ${versionResult.fileCount} file(s) passed integrity check.`, 'ok')
        setStep(3, 'done')
      } catch (e) {
        addLog(`ERROR: Verification failed — ${String(e)}`, 'warn')
        setPhase('error')
        setError(String(e))
        return
      }

      // Step 5: Finalize
      setStep(4, 'active')
      addLog(`Modpack ${versionResult.remoteVersion} installed successfully.`, 'ok')
      addLog('Ready for deployment.', 'info')
      setStep(4, 'done')
      setPhase('complete')
    }

    run()

    return () => {
      unlistenRef.current?.()
    }
  }, [addLog, setStep])

  const handleRetry = useCallback(() => {
    cancelledRef.current = false
    startedRef.current = false
    setError(null)
    setVerifyFailed([])
    setLogLines([])
    setProgress(0)
    setPhase('checking')
    setSteps([
      { label: 'update.step.verify', status: 'pending' },
      { label: 'update.step.prune', status: 'pending' },
      { label: 'update.step.download', status: 'pending' },
      { label: 'update.step.integrity', status: 'pending' },
      { label: 'update.step.finalize', status: 'pending' },
    ])
    // Re-trigger the effect — a key trick is to remount, but for simplicity we reload
    window.location.reload()
  }, [])

  const handleCancel = useCallback(async () => {
    cancelledRef.current = true
    if (isTauri()) {
      try {
        await invoke('cancel_download')
      } catch {
        /* ignore */
      }
    }
  }, [])

  const statusText = () => {
    switch (phase) {
      case 'checking':
        return t('update.checking')
      case 'downloading':
        return t('update.downloading')
      case 'verifying':
        return t('update.step.integrity')
      case 'complete':
        return t('update.uptodate')
      case 'uptodate':
        return t('update.uptodate')
      case 'error':
        return t('settings.saveError')
      default:
        return t('update.updating')
    }
  }

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="relative h-full p-5 flex flex-col min-h-0">
        <OperationBar
          label={t('update.packageSync')}
          status={statusText()}
          statusColor={phase === 'error' ? '#c98b8b' : '#F5A524'}
        />

        <div className="flex items-start justify-between gap-6 min-w-0">
          <div>
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-1.5">
              <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
                {OPERATION_NAME}
              </h1>
              <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
                {t('update.patch', { v: manifestVersion || '...' })}
              </span>
            </div>
            <p className="text-[12px] leading-snug text-[#C7AE86] max-w-[520px]">
              {error ? error.slice(0, 200) : t('update.description')}
            </p>
          </div>
          <div className="shrink-0 border border-[#2A2116] bg-[#0B0906] px-4 py-3 w-[240px]">
            <div className="text-[10px] tracking-[0.16em] text-[#8E7A5E]">{t('update.mirror')}</div>
            <div className="mt-2 tracking-[0.18em] text-[13px] text-neutral-100">CDN-FRA-02</div>
            <div className="mt-2 text-[11px] tracking-[0.22em] text-[#82D66B]">
              ▸ {phase === 'downloading' ? speed : '—'}
            </div>
          </div>
        </div>

        <GlowPanel className="p-4">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-4">
              {phase === 'downloading' || phase === 'verifying' ? (
                <button
                  onClick={handleCancel}
                  className="relative h-[64px] w-[230px] shrink-0 overflow-hidden border border-[#3a2828] bg-[#1a0e0e] hover:border-[#7a3838] transition-colors"
                >
                  <span className="h-full flex items-center justify-center gap-4">
                    <Pause size={18} className="text-[#c98b8b]" />
                    <span className="flex flex-col items-start leading-none">
                      <span className="tracking-[0.2em] text-[17px] text-[#c98b8b]">CANCEL</span>
                      <span className="tracking-[0.14em] text-[9px] text-[#7a3838] mt-1 text-left">
                        {t('update.inProgress')}
                      </span>
                    </span>
                  </span>
                </button>
              ) : phase === 'complete' || phase === 'uptodate' ? (
                <DeployButton label="PLAY" sub={t('main.enterBattlefield')} />
              ) : phase === 'error' ? (
                <RetryButton onClick={handleRetry} label={t('update.retry')} />
              ) : (
                <LockedButton lockedLabel="..." subLabel={t('update.checking')} />
              )}
              {phase === 'downloading' && (
                <button
                  onClick={handleCancel}
                  className="h-[40px] w-[40px] grid place-items-center border border-[#2A2116] hover:border-[#8A571C] text-[#C7AE86] hover:text-[#F3E7D0] transition-colors"
                >
                  <Pause size={14} />
                </button>
              )}
            </div>

            <div className="text-right">
              <div className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
                {t('update.completion')}
              </div>
              <div className="tracking-[0.04em] text-[#F3E7D0] leading-none mt-1.5 text-[44px]">
                {phase === 'checking' ? (
                  <LoaderCircle size={32} className="animate-spin text-[#8E7A5E] inline-block" />
                ) : (
                  <>
                    {progress.toFixed(1)}
                    <span className="text-[#8E7A5E] text-[20px]">%</span>
                  </>
                )}
              </div>
            </div>
          </div>

          <ProgressBar progress={progress} />

          <div className="mt-4 grid grid-cols-4 gap-px bg-[#18130D] border border-[#2A2116] min-w-0">
            <SubStat
              icon={<FileBox size={13} />}
              label={t('update.currentFile')}
              value={currentFile || '—'}
              mono
            />
            <SubStat
              icon={<HardDrive size={13} />}
              label={t('update.transferred')}
              value={`${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}`}
            />
            <SubStat
              icon={<Download size={13} />}
              label={t('update.remaining')}
              value={
                phase === 'downloading'
                  ? `${fileIdx}/${fileCount || '...'}`
                  : totalBytes > 0
                    ? `${fileCount} files`
                    : '—'
              }
            />
            <SubStat
              icon={<Zap size={13} />}
              label={t('update.throughput')}
              value={phase === 'downloading' ? speed : '—'}
              accent
            />
          </div>

          <div className="mt-3 flex items-start justify-between gap-4 text-[10px] tracking-[0.16em]">
            <div className="flex min-w-0 items-start gap-2">
              <StatusDot
                color={
                  phase === 'error'
                    ? '#c98b8b'
                    : phase === 'complete' || phase === 'uptodate'
                      ? '#82D66B'
                      : '#F5A524'
                }
                pulse={phase === 'downloading'}
              />
              <span
                className={`leading-snug ${
                  phase === 'error'
                    ? 'text-[#c98b8b]'
                    : phase === 'complete' || phase === 'uptodate'
                      ? 'text-[#82D66B]'
                      : 'text-[#F5A524]'
                }`}
              >
                {statusText()}
              </span>
            </div>
            <span className="shrink-0 text-right text-[#8E7A5E]">
              {verifyFailed.length > 0
                ? `${verifyFailed.length} FAILED`
                : phase === 'complete' || phase === 'uptodate'
                  ? 'SHA256 VERIFIED'
                  : t('update.integrity')}
            </span>
          </div>
        </GlowPanel>

        <div className="mt-3 min-h-0 flex-1 grid grid-cols-[minmax(0,1.35fr)_minmax(300px,1fr)] gap-px bg-[#18130D] border border-[#2A2116] overflow-hidden">
          <div className="bg-[#0B0906] p-4 min-h-0 overflow-hidden">
            <SectionHeader label={t('update.opLog')} code="SYNC-PHASE-2" />
            <div className="mt-3 font-mono text-[11px] leading-relaxed text-[#C7AE86] space-y-1 overflow-y-auto max-h-[calc(100%-2rem)]">
              {logLines.length === 0 && (
                <LogLine ts="--:--:--" tone="dim" msg="Waiting for connection..." />
              )}
              {logLines.map((line, i) => (
                <LogLine key={i} ts={line.ts} tone={line.tone} msg={line.msg} />
              ))}
            </div>
          </div>
          <div className="bg-[#0B0906] p-4 flex flex-col min-h-0">
            <SectionHeader label={t('update.steps')} code="SEQ" />
            <div className="mt-3 flex flex-col gap-2 flex-1">
              {steps.map((step) => (
                <Step key={step.label} label={t(step.label)} status={step.status} t={t} />
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function DeployButton({ label, sub }: { label: string; sub: string }) {
  return (
    <button className="group relative h-[64px] w-[230px] shrink-0 overflow-hidden border border-[#F5A524]/50 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524] transition-all"
      style={{
        boxShadow: 'inset 0 0 0 1px rgba(245,165,36,0.1), 0 0 40px -8px rgba(245,165,36,0.45)',
      }}
    >
      <span className="absolute top-0 left-0 w-3 h-3 border-l border-t border-[#F5A524]" />
      <span className="absolute top-0 right-0 w-3 h-3 border-r border-t border-[#F5A524]" />
      <span className="absolute bottom-0 left-0 w-3 h-3 border-l border-b border-[#F5A524]" />
      <span className="absolute bottom-0 right-0 w-3 h-3 border-r border-b border-[#F5A524]" />
      <span className="absolute inset-0 bg-[#F5A524]/0 group-hover:bg-[#F5A524]/10 transition-colors" />
      <span className="relative h-full flex items-center justify-center gap-4">
        <Play size={18} className="text-[#F3E7D0] fill-[#F3E7D0]" />
        <span className="flex flex-col items-start leading-none">
          <span className="tracking-[0.26em] text-[17px] text-[#F3E7D0]">{label}</span>
          <span className="tracking-[0.16em] text-[9px] text-[#C7AE86] mt-1 text-left">{sub}</span>
        </span>
      </span>
    </button>
  )
}

function RetryButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      onClick={onClick}
      className="relative h-[64px] w-[230px] shrink-0 overflow-hidden border border-[#8A571C] bg-[#1a1008] hover:border-[#F5A524] transition-colors"
    >
      <span className="h-full flex items-center justify-center gap-4">
        <RefreshCwIcon />
        <span className="flex flex-col items-start leading-none">
          <span className="tracking-[0.2em] text-[17px] text-[#F5A524]">{label}</span>
          <span className="tracking-[0.14em] text-[9px] text-[#8E7A5E] mt-1 text-left">
            RETRY DEPLOYMENT
          </span>
        </span>
      </span>
    </button>
  )
}

function RefreshCwIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-[#F5A524]">
      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
      <path d="M3 21v-5h5" />
    </svg>
  )
}

function ProgressBar({ progress }: { progress: number }) {
  return (
    <div className="relative h-2.5 bg-[#0B0906] border border-[#2A2116] overflow-hidden">
      <div
        className="h-full bg-gradient-to-r from-[#8A571C] via-[#F5A524] to-[#FFC861] transition-all duration-200"
        style={{ width: `${progress}%`, boxShadow: '0 0 16px rgba(245,165,36,0.55)' }}
      />
      <div className="absolute inset-0 pointer-events-none flex">
        {Array.from({ length: 40 }).map((_, i) => (
          <span key={i} className="flex-1 border-r border-[#070604]/60" />
        ))}
      </div>
    </div>
  )
}

function LockedButton({ lockedLabel, subLabel }: { lockedLabel: string; subLabel: string }) {
  return (
    <button
      disabled
      className="relative h-[64px] w-[230px] shrink-0 overflow-hidden border border-[#2A2116] bg-[#0B0906] opacity-60 cursor-not-allowed"
    >
      <span className="absolute top-0 left-0 w-3 h-3 border-l border-t border-[#3A2C1D]" />
      <span className="absolute top-0 right-0 w-3 h-3 border-r border-t border-[#3A2C1D]" />
      <span className="absolute bottom-0 left-0 w-3 h-3 border-l border-b border-[#3A2C1D]" />
      <span className="absolute bottom-0 right-0 w-3 h-3 border-r border-b border-[#3A2C1D]" />
      <span className="h-full flex items-center justify-center gap-4">
        <LoaderCircle size={18} className="text-[#5E5040] animate-spin" />
        <span className="flex flex-col items-start leading-none">
          <span className="tracking-[0.2em] text-[17px] text-[#8E7A5E]">{lockedLabel}</span>
          <span className="tracking-[0.14em] text-[9px] text-[#5E5040] mt-1 text-left">
            {subLabel}
          </span>
        </span>
      </span>
    </button>
  )
}

function SubStat({
  icon,
  label,
  value,
  mono,
  accent,
}: {
  icon: ReactNode
  label: string
  value: string
  mono?: boolean
  accent?: boolean
}) {
  return (
    <div className="bg-[#0B0906] p-4 min-w-0">
      <div className="flex items-center gap-2 text-[9px] tracking-[0.14em] text-[#8E7A5E]">
        {icon} {label}
      </div>
      <div
        className={`mt-2 text-[12px] tracking-[0.12em] truncate ${
          accent ? 'text-[#82D66B]' : 'text-neutral-100'
        } ${mono ? 'font-mono' : ''}`}
      >
        {value}
      </div>
    </div>
  )
}

const LOG_TONE: Record<'ok' | 'info' | 'dim' | 'warn', string> = {
  ok: 'text-[#82D66B]',
  info: 'text-[#F5A524]',
  dim: 'text-[#8E7A5E]',
  warn: 'text-[#c98b8b]',
}

function LogLine({ ts, tone, msg }: { ts: string; tone: 'ok' | 'info' | 'dim' | 'warn'; msg: string }) {
  return (
    <div className="flex gap-4 min-w-0">
      <span className="text-[#5E5040] shrink-0">[{ts}]</span>
      <span className={LOG_TONE[tone]}>{msg}</span>
    </div>
  )
}

const STEP_DOT: Record<StepStatus, string> = {
  done: 'bg-[#82D66B] shadow-[0_0_6px_rgba(130,214,107,0.5)]',
  active: 'bg-[#F5A524] animate-pulse shadow-[0_0_10px_rgba(245,165,36,0.7)]',
  pending: 'bg-[#3A2C1D]',
}
const STEP_TEXT: Record<StepStatus, string> = {
  done: 'text-[#C7AE86]',
  active: 'text-[#F3E7D0]',
  pending: 'text-[#8E7A5E]',
}
const STEP_TAG: Record<StepStatus, TKey> = {
  done: 'update.step.done',
  active: 'update.step.running',
  pending: 'update.step.queued',
}

function Step({ label, status, t }: { label: string; status: StepStatus; t: TFunction }) {
  return (
    <div className="flex items-start justify-between gap-3 border-b border-dashed border-[#18130D] pb-2 last:border-0">
      <div className="flex items-center gap-3">
        <span className={`size-2 rounded-full ${STEP_DOT[status]}`} />
        <span className={`text-[11px] tracking-[0.12em] ${STEP_TEXT[status]}`}>{label}</span>
      </div>
      <span className="shrink-0 text-[9px] tracking-[0.16em] text-[#8E7A5E]">
        {t(STEP_TAG[status])}
      </span>
    </div>
  )
}
