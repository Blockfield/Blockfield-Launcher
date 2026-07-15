import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  Play,
  Wifi,
  Users,
  Activity,
  ShieldCheck,
  Flag,
  Swords,
  Truck,
  Crosshair,
  ChevronRight,
  RefreshCw,
  LoaderCircle,
} from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { GlowPanel, OperationBar, SectionHeader, StatusDot } from './ui-bits'
import { useI18n, type TKey } from '../i18n'
import { OPERATION_NAME, SERVER_IP } from '../constants'
import { listenDownloadProgress, listenLauncherStatus } from '../../lib/events'
import {
  fetchServerStatus,
  type DownloadProgress,
  type ServerStatus as ServerStatusData,
  type VersionCheckResult,
} from '../../lib/api'
import { contentText, localizedContentText, useLauncherContent } from '../../lib/content'

type Tone = 'ok' | 'muted' | 'warn'

type FallbackFeature = { icon: ReactNode; title: TKey; desc: TKey }

const FEATURES: FallbackFeature[] = [
  { icon: <Flag size={14} />, title: 'main.feat.capture', desc: 'main.feat.captureDesc' },
  { icon: <Swords size={14} />, title: 'main.feat.classes', desc: 'main.feat.classesDesc' },
  { icon: <Truck size={14} />, title: 'main.feat.vehicles', desc: 'main.feat.vehiclesDesc' },
  { icon: <Crosshair size={14} />, title: 'main.feat.battles', desc: 'main.feat.battlesDesc' },
]

const FEATURE_ICONS: Record<string, ReactNode> = {
  flag: <Flag size={14} />,
  swords: <Swords size={14} />,
  truck: <Truck size={14} />,
  crosshair: <Crosshair size={14} />,
}

type FeedTone = 'amber' | 'green' | 'sand'
type FallbackFeedEntry = {
  tagKey: TKey
  tone: FeedTone
  date: string
  titleKey: TKey
  bodyKey: TKey
  vars?: Record<string, string>
}
type FeedEntry = { tag: string; tone: FeedTone; date: string; title: string; body: string }

const FEED: FallbackFeedEntry[] = [
  {
    tagKey: 'main.tag.patch',
    tone: 'amber',
    date: '06.07',
    titleKey: 'main.feed.patch.title',
    bodyKey: 'main.feed.patch.body',
    vars: { v: '...' },
  },
  {
    tagKey: 'main.tag.event',
    tone: 'green',
    date: '06.05',
    titleKey: 'main.feed.event.title',
    bodyKey: 'main.feed.event.body',
  },
  {
    tagKey: 'main.tag.ops',
    tone: 'sand',
    date: '06.02',
    titleKey: 'main.feed.ops.title',
    bodyKey: 'main.feed.ops.body',
  },
]

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

export function MainScreen({ onPlay }: { onPlay: () => void }) {
  const { lang, t } = useI18n()
  const content = useLauncherContent()
  const [versionInfo, setVersionInfo] = useState<VersionCheckResult | null>(null)
  const [checking, setChecking] = useState(true)
  const [checkError, setCheckError] = useState<string | null>(null)
  const [launching, setLaunching] = useState(false)
  const [launchStatus, setLaunchStatus] = useState('')
  const [launchError, setLaunchError] = useState<string | null>(null)
  const [launchProgress, setLaunchProgress] = useState<number | null>(null)
  const [serverStatus, setServerStatus] = useState<ServerStatusData | null>(null)

  const handleDeploy = useCallback(async () => {
    // Don't allow deploy until version check completes
    if (!versionInfo) return
    // Update needed OR Java not ready OR Forge not installed → go to update screen
    if (versionInfo.needsUpdate || !versionInfo.javaOk || !versionInfo.forgeOk) {
      onPlay()
      return
    }
    // Confirmed up to date AND Java OK AND Forge installed → launch
    if (isTauri()) {
      setLaunching(true)
      setLaunchError(null)
      setLaunchStatus(t('main.launching'))
      setLaunchProgress(null)
      let unlistenStatus: UnlistenFn | null = null
      let unlistenProgress: UnlistenFn | null = null
      try {
        unlistenStatus = await listenLauncherStatus((status) => {
          setLaunchStatus(status.message)
        })
        unlistenProgress = await listenDownloadProgress((p: DownloadProgress) => {
          if (p.totalBytesAll > 0) {
            setLaunchProgress(Math.min(100, (p.totalBytesDownloaded / p.totalBytesAll) * 100))
          }
          if (p.filePath) setLaunchStatus(p.filePath)
        })
        await invoke('launch_game')
        setLaunchStatus(t('main.gameStarted'))
      } catch (e) {
        console.error('Launch failed:', e)
        setLaunchError(String(e))
        setLaunchStatus('')
      } finally {
        unlistenStatus?.()
        unlistenProgress?.()
        setLaunching(false)
      }
    }
  }, [versionInfo, onPlay, t])

  const checkVersion = useCallback(async () => {
    if (!isTauri()) return
    setChecking(true)
    setCheckError(null)
    setLaunchError(null)
    try {
      const result = await invoke<VersionCheckResult>('check_modpack_version')
      setVersionInfo(result)
    } catch (e) {
      console.error('Failed to check modpack version:', e)
      setCheckError(String(e))
    } finally {
      setChecking(false)
    }
  }, [])

  // Check version on mount (if running in Tauri)
  useEffect(() => {
    if (!isTauri()) return
    // eslint-disable-next-line react-hooks/set-state-in-effect -- intentional sync: this is an external system subscription (Tauri IPC)
    checkVersion()
  }, [checkVersion])

  useEffect(() => {
    const refresh = () =>
      fetchServerStatus()
        .then(setServerStatus)
        .catch(() => setServerStatus(null))
    refresh()
    const interval = window.setInterval(refresh, 10_000)
    return () => window.clearInterval(interval)
  }, [])

  // Derive display values from version check result
  const isChecked = versionInfo !== null
  const needsUpdate = versionInfo?.needsUpdate ?? false
  const needsSetup = isChecked && (needsUpdate || !versionInfo.javaOk || !versionInfo.forgeOk)
  const upToDate = isChecked && !needsSetup
  const installedVersion = versionInfo?.installedVersion ?? '...'
  const latestVersion = versionInfo?.remoteVersion ?? '...'
  const totalSize = versionInfo?.totalSize ? `${(versionInfo.totalSize / 1e9).toFixed(1)} GB` : '—'
  const launchProgressText = launchProgress === null ? '' : ` · ${launchProgress.toFixed(0)}%`
  const launchSub = launching
    ? `${launchStatus || t('main.launching')}${launchProgressText}`.slice(0, 64)
    : launchError
      ? launchError.slice(0, 64)
      : !isChecked
        ? t('main.checking')
        : needsSetup
          ? t('main.updateAvailable')
          : t('main.enterBattlefield')
  const launchLabel = launching
    ? t('main.launching')
    : launchError
      ? t('main.launchFailed')
      : !isChecked
        ? '...'
        : needsSetup
          ? t('nav.updates')
          : t('nav.deploy')

  // Modpack status line
  const modpackStatus: { value: string; tone: Tone } = checking
    ? { value: t('main.checking'), tone: 'muted' }
    : checkError
      ? { value: 'OFFLINE', tone: 'warn' }
      : upToDate
        ? { value: t('main.upToDate'), tone: 'ok' }
        : needsSetup
          ? { value: t('main.updateAvailable'), tone: 'warn' }
          : { value: t('main.upToDate'), tone: 'muted' }
  const operationName =
    localizedContentText(
      content,
      lang,
      'content.operationName',
      'operationName',
      'operation_name',
    ) ?? OPERATION_NAME
  const season = localizedContentText(content, lang, 'content.season', 'season') ?? t('main.season')
  const description =
    localizedContentText(content, lang, 'content.description', 'description') ??
    t('main.description')
  const serverName =
    localizedContentText(content, lang, 'content.serverName', 'serverName', 'server_name') ??
    t('main.serverName')
  const serverIp = contentText(content, 'serverIp', 'server_ip') ?? SERVER_IP
  const players = serverStatus?.playersOnline?.toString() ?? '—'
  const ping = serverStatus?.serverLatencyMs?.toString() ?? '—'
  const region = serverStatus?.regionCode ?? '—'
  const features =
    content?.features?.flatMap((feature, index) =>
      feature.title && feature.desc
        ? [
            {
              icon: featureIcon(feature.icon),
              title: localizedContentText(content, lang, `feature.${index}.title`) ?? feature.title,
              desc: localizedContentText(content, lang, `feature.${index}.desc`) ?? feature.desc,
            },
          ]
        : [],
    ) ?? []
  const displayFeatures =
    features.length > 0
      ? features
      : FEATURES.map((feature) => ({
          icon: feature.icon,
          title: t(feature.title),
          desc: t(feature.desc),
        }))
  const feed =
    content?.feed?.flatMap((entry, index) =>
      entry.title && entry.body
        ? [
            {
              tag: localizedContentText(content, lang, `feed.${index}.tag`) ?? entry.tag ?? '',
              tone: feedTone(entry.tone),
              date: entry.date ?? '',
              title: localizedContentText(content, lang, `feed.${index}.title`) ?? entry.title,
              body: localizedContentText(content, lang, `feed.${index}.body`) ?? entry.body,
            },
          ]
        : [],
    ) ?? []
  const displayFeed =
    feed.length > 0
      ? feed
      : FEED.map((entry) => ({
          tag: t(entry.tagKey),
          tone: entry.tone,
          date: entry.date,
          title: t(entry.titleKey, entry.vars ? { ...entry.vars, v: latestVersion } : undefined),
          body: t(entry.bodyKey),
        }))

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="relative h-full grid grid-cols-[minmax(0,1fr)_340px] gap-0">
        <section className="relative p-5 flex flex-col min-h-0">
          <OperationBar label={t('main.operation')} status={t('main.active')} />

          <div className="flex min-w-0 items-start justify-between gap-6">
            <div className="max-w-[560px]">
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-1.5">
                <h1 className="tracking-[0.06em] text-[26px] leading-none text-neutral-50">
                  {operationName}
                </h1>
                <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">{season}</span>
              </div>
              <p className="text-[12px] leading-snug text-[#C7AE86] max-w-[520px]">{description}</p>
            </div>

            <ServerStatus
              serverName={serverName}
              serverIp={serverStatus ? `${serverStatus.host}:${serverStatus.port}` : serverIp}
              online={serverStatus?.online === true}
            />
          </div>

          {/* PLAY zone */}
          <GlowPanel className="p-4">
            <div className="flex flex-wrap items-center justify-between gap-5">
              <div className="flex min-w-0 items-center gap-4">
                <DeployButton
                  onPlay={handleDeploy}
                  disabled={!isChecked || launching}
                  label={launchLabel}
                  sub={launchSub}
                  busy={launching}
                />
                <div className="flex flex-col gap-2 pl-2">
                  <Stat
                    label={t('main.modpack')}
                    value={modpackStatus.value}
                    tone={modpackStatus.tone}
                  />
                  <Stat label={t('main.auth')} value={t('main.verified')} tone="ok" />
                  <Stat label={t('main.queue')} value={t('main.none')} tone="muted" />
                </div>
              </div>

              <div className="flex shrink-0 items-center gap-4 text-right">
                <Metric
                  icon={<Users size={14} />}
                  label={t('main.players')}
                  value={players}
                  sub={serverStatus?.playersMax ? `/ ${serverStatus.playersMax}` : ''}
                />
                <span className="h-10 w-px bg-[#18130D]" />
                <Metric
                  icon={<Activity size={14} />}
                  label={t('main.ping')}
                  value={ping}
                  sub="MS"
                />
                <span className="h-10 w-px bg-[#18130D]" />
                <Metric
                  icon={<Wifi size={14} />}
                  label={t('main.region')}
                  value={region}
                  sub={serverStatus?.locationName ?? ''}
                />
              </div>
            </div>
          </GlowPanel>

          {/* Briefing + Modpack */}
          <div className="mt-4 grid grid-cols-[minmax(0,1.35fr)_minmax(270px,1fr)] gap-px bg-[#18130D] border border-[#2A2116] flex-1 min-h-0">
            <div className="bg-[#0B0906] p-4 min-h-0">
              <SectionHeader label={t('main.briefing')} code="BRF-001" />
              <div className="grid grid-cols-2 gap-x-5 gap-y-3 mt-3">
                {displayFeatures.map((f) => (
                  <FeatureItem key={f.title} icon={f.icon} title={f.title} desc={f.desc} />
                ))}
              </div>
            </div>

            <div className="bg-[#0B0906] p-4 flex flex-col min-h-0">
              <SectionHeader label={t('main.modpackStatus')} code="PKG-LIVE" />
              <div className="mt-3 flex flex-col gap-2 flex-1">
                <Row label={t('main.installed')} value={installedVersion} />
                <Row label={t('main.latest')} value={latestVersion} highlight={needsUpdate} />
                <Row label={t('main.size')} value={totalSize} />
                <Row
                  label={t('main.autoUpdate')}
                  value={needsSetup ? t('main.updateAvailable') : t('main.upToDate')}
                  highlight={needsSetup}
                />
                {checkError && (
                  <div className="text-[10px] tracking-[0.12em] text-[#c98b8b] mt-1">
                    {checkError.slice(0, 120)}
                  </div>
                )}
              </div>
              <button
                onClick={checkVersion}
                disabled={checking}
                className="mt-3 h-8 border border-[#2A2116] hover:border-[#8A571C] text-[10px] tracking-[0.16em] text-[#C7AE86] hover:text-[#F3E7D0] flex items-center justify-center gap-2 transition-colors disabled:opacity-50"
              >
                {checking ? (
                  <LoaderCircle size={12} className="animate-spin" />
                ) : (
                  <RefreshCw size={12} />
                )}
                {checking ? t('main.checking') : t('nav.updates')}
              </button>
            </div>
          </div>
        </section>

        {/* Side rail */}
        <aside className="relative border-l border-[#18130D] bg-[#0B0906]/80 p-4 flex flex-col min-h-0">
          <SectionHeader label={t('main.fieldReport')} code="OPS-LOG" />

          <div className="mt-3 flex-1 min-h-0 flex flex-col gap-px bg-[#18130D] border border-[#2A2116] overflow-hidden">
            {displayFeed.map((entry) => (
              <FeedItem key={`${entry.date}:${entry.title}`} entry={entry} />
            ))}
          </div>

          <button className="mt-3 min-h-8 text-left text-[10px] tracking-[0.16em] text-[#8E7A5E] hover:text-[#F3E7D0] flex items-center justify-between gap-2 border-t border-[#18130D] pt-3">
            <span>{t('main.viewLog')}</span>
            <ChevronRight size={12} />
          </button>
        </aside>
      </div>
    </div>
  )
}

function DeployButton({
  onPlay,
  label,
  sub,
  disabled,
  busy,
}: {
  onPlay: () => void
  label: string
  sub: string
  disabled?: boolean
  busy?: boolean
}) {
  return (
    <button
      onClick={onPlay}
      disabled={disabled}
      className={`group relative h-[64px] w-[230px] shrink-0 overflow-hidden border transition-all ${
        disabled
          ? 'border-[#2A2116] bg-[#0B0906] opacity-50 cursor-not-allowed'
          : 'border-[#F5A524]/50 bg-gradient-to-b from-[#2A2116] to-[#11100D] hover:border-[#F5A524]'
      }`}
      style={
        disabled
          ? undefined
          : {
              boxShadow:
                'inset 0 0 0 1px rgba(245,165,36,0.1), 0 0 40px -8px rgba(245,165,36,0.45)',
            }
      }
    >
      <span
        className={`absolute top-0 left-0 w-3 h-3 border-l border-t ${disabled ? 'border-[#3A2C1D]' : 'border-[#F5A524]'}`}
      />
      <span
        className={`absolute top-0 right-0 w-3 h-3 border-r border-t ${disabled ? 'border-[#3A2C1D]' : 'border-[#F5A524]'}`}
      />
      <span
        className={`absolute bottom-0 left-0 w-3 h-3 border-l border-b ${disabled ? 'border-[#3A2C1D]' : 'border-[#F5A524]'}`}
      />
      <span
        className={`absolute bottom-0 right-0 w-3 h-3 border-r border-b ${disabled ? 'border-[#3A2C1D]' : 'border-[#F5A524]'}`}
      />
      <span className="absolute inset-0 bg-[#F5A524]/0 group-hover:bg-[#F5A524]/10 transition-colors" />
      <span className="relative h-full flex items-center justify-center gap-4">
        {busy ? (
          <LoaderCircle size={18} className="animate-spin text-[#F3E7D0]" />
        ) : (
          <Play
            size={18}
            className={disabled ? 'text-[#5E5040] fill-[#5E5040]' : 'text-[#F3E7D0] fill-[#F3E7D0]'}
          />
        )}
        <span className="flex flex-col items-start leading-none">
          <span className="tracking-[0.26em] text-[17px] text-[#F3E7D0]">{label}</span>
          <span className="tracking-[0.16em] text-[9px] text-[#C7AE86] mt-1 text-left">{sub}</span>
        </span>
      </span>
    </button>
  )
}

function ServerStatus({
  serverName,
  serverIp,
  online,
}: {
  serverName: string
  serverIp: string
  online: boolean
}) {
  const { t } = useI18n()
  return (
    <div className="shrink-0 border border-[#2A2116] bg-[#0B0906] px-4 py-3 w-[240px]">
      <div className="flex items-start justify-between gap-2">
        <span className="text-[10px] tracking-[0.14em] text-[#8E7A5E]">{t('main.server')}</span>
        <span
          className={`flex items-center gap-1.5 text-[10px] tracking-[0.16em] ${online ? 'text-[#82D66B]' : 'text-[#E36A5D]'}`}
        >
          <StatusDot pulse={online} />
          {online ? t('main.online') : 'UNAVAILABLE'}
        </span>
      </div>
      <div className="mt-2 flex items-center gap-2">
        <ShieldCheck size={14} className="text-[#F5A524]" />
        <span className="tracking-[0.18em] text-[13px] text-neutral-100">{serverName}</span>
      </div>
      <div className="mt-2 text-[10px] tracking-[0.22em] text-[#8E7A5E]">{serverIp}</div>
    </div>
  )
}

const TONE_COLOR: Record<Tone, string> = {
  ok: 'text-[#82D66B]',
  warn: 'text-[#F5A524]',
  muted: 'text-[#C7AE86]',
}

function Stat({ label, value, tone }: { label: string; value: string; tone: Tone }) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-[9px] tracking-[0.16em] text-[#8E7A5E] w-20 shrink-0">{label}</span>
      <span className={`text-[10px] tracking-[0.16em] ${TONE_COLOR[tone]}`}>▸ {value}</span>
    </div>
  )
}

function Metric({
  icon,
  label,
  value,
  sub,
}: {
  icon: ReactNode
  label: string
  value: string
  sub: string
}) {
  return (
    <div className="flex flex-col items-end gap-1 min-w-[58px]">
      <span className="flex items-center gap-1.5 text-[9px] tracking-[0.16em] text-[#8E7A5E]">
        {icon} {label}
      </span>
      <span className="tracking-[0.08em] text-[22px] leading-none text-neutral-50">
        {value}
        <span className="text-[10px] tracking-[0.16em] text-[#8E7A5E] ml-1">{sub}</span>
      </span>
    </div>
  )
}

function FeatureItem({ icon, title, desc }: { icon: ReactNode; title: string; desc: string }) {
  return (
    <div className="flex gap-3">
      <div className="mt-0.5 size-7 grid place-items-center border border-[#2A2116] bg-[#11100D] text-[#F5A524]">
        {icon}
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] tracking-[0.12em] text-neutral-100">{title}</span>
        <span className="text-[11px] leading-snug text-[#8E7A5E]">{desc}</span>
      </div>
    </div>
  )
}

function Row({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="flex items-center justify-between border-b border-dashed border-[#18130D] pb-2 last:border-0">
      <span className="text-[10px] tracking-[0.14em] text-[#8E7A5E]">{label}</span>
      <span
        className={`text-right text-[11px] tracking-[0.12em] ${highlight ? 'text-[#F5A524]' : 'text-neutral-100'}`}
      >
        {value}
      </span>
    </div>
  )
}

const FEED_TAG_CLASS: Record<FeedTone, string> = {
  amber: 'text-[#F5A524] border-[#8A571C]',
  green: 'text-[#82D66B] border-[#3a5a30]',
  sand: 'text-[#C7AE86] border-[#3A2C1D]',
}

function FeedItem({ entry }: { entry: FeedEntry }) {
  return (
    <details className="group bg-[#0B0906] p-4 hover:bg-[#11100D] transition-colors">
      <summary className="list-none cursor-pointer [&::-webkit-details-marker]:hidden">
        <div className="flex items-start justify-between gap-2">
          <span
            className={`shrink-0 text-[9px] tracking-[0.16em] border px-1.5 py-0.5 ${FEED_TAG_CLASS[entry.tone]}`}
          >
            {entry.tag}
          </span>
          <span className="text-[10px] tracking-[0.24em] text-[#5E5040]">{entry.date}</span>
        </div>
        <span className="mt-2 flex items-center justify-between gap-2 text-[12px] tracking-[0.04em] text-neutral-100">
          {entry.title}
          <ChevronRight size={12} className="shrink-0 transition-transform group-open:rotate-90" />
        </span>
      </summary>
      <p className="mt-2 text-[11px] leading-snug text-[#8E7A5E]">{entry.body}</p>
    </details>
  )
}

function featureIcon(icon?: string) {
  return (icon && FEATURE_ICONS[icon]) || <Flag size={14} />
}

function feedTone(tone?: string): FeedTone {
  return tone === 'green' || tone === 'sand' ? tone : 'amber'
}
