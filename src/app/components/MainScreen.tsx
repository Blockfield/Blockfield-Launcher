import { GAME_MAINTENANCE, GAME_UPDATING, RoomList } from './RoomList'
import { useGameState, gameStateLabel, launchGame } from '../../lib/game-state'
import { checkModpack, useModpackVersion } from '../../lib/modpack-check'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useState, useRef, type ReactNode } from 'react'
import { Select } from './Select'
import {
  Play,
  Gamepad2,
  Wifi,
  Users,
  Activity,
  ShieldCheck,
  Flag,
  Swords,
  Truck,
  Crosshair,
  RefreshCw,
  LoaderCircle,
} from 'lucide-react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { GridBackdrop, TopoBackdrop } from './Backdrop'
import { ErrorDetail, GlowPanel, OperationBar, SectionHeader, StatusDot } from './ui-bits'
import { friendlyError } from '../../lib/errors'
import { useI18n, type TKey } from '../i18n'
import { OPERATION_NAME, SERVER_IP } from '../constants'
import { listenDownloadProgress, listenLauncherStatus } from '../../lib/events'
import {
  fetchServerStatus,
  type DownloadProgress,
  type ServerStatus as ServerStatusData,
} from '../../lib/api'
import { contentText, localizedContentText, useLauncherContent } from '../../lib/content'

type Tone = 'ok' | 'muted' | 'warn'

type FallbackFeature = { icon: ReactNode; title: TKey; desc: TKey }

const FEATURES: FallbackFeature[] = [
  { icon: <Flag size={14} />, title: 'main.feat.capture', desc: 'main.feat.captureDesc' },
  { icon: <Swords size={14} />, title: 'main.feat.classes', desc: 'main.feat.classesDesc' },
  { icon: <Crosshair size={14} />, title: 'main.feat.loadout', desc: 'main.feat.loadoutDesc' },
  { icon: <Gamepad2 size={14} />, title: 'main.feat.menu', desc: 'main.feat.menuDesc' },
]

const FEATURE_ICONS: Record<string, ReactNode> = {
  flag: <Flag size={14} />,
  swords: <Swords size={14} />,
  truck: <Truck size={14} />,
  crosshair: <Crosshair size={14} />,
}

/** Whether we're running inside Tauri (vs browser dev). */
const isTauri = () => '__TAURI_INTERNALS__' in window

const PROFILE_OPTIONS = [
  { value: 'game', label: 'Игра' },
  { value: 'workshop', label: 'Мастерская' },
]

export function ProfileSelect({
  value,
  disabled,
  onChange,
}: {
  value: 'game' | 'workshop'
  disabled: boolean
  onChange: (profile: 'game' | 'workshop') => void
}) {
  return (
    <Select
      id="client-profile"
      ariaLabel="Профиль"
      value={value}
      options={PROFILE_OPTIONS}
      disabled={disabled}
      className="min-w-[140px]"
      onChange={(next) => onChange(next as 'game' | 'workshop')}
    />
  )
}

export function MainScreen({
  onPlay,
  profile = 'game',
  profileChanging = false,
  profileBlocked = false,
  profileError,
  onSelectProfile,
}: {
  onPlay: () => void
  profile?: 'game' | 'workshop'
  profileChanging?: boolean
  profileBlocked?: boolean
  profileError?: string | null
  onSelectProfile?: (profile: 'game' | 'workshop') => void
}) {
  const { lang, t } = useI18n()
  const game = useGameState()
  const gameBusy = game.phase !== 'idle'
  const content = useLauncherContent()
  // Single source of truth (src/lib/modpack-check.ts): "Обновления" reads the same store,
  // so the two tabs can't show different installed/remote versions.
  const versionInfo = useModpackVersion()
  const [checking, setChecking] = useState(true)
  const [checkError, setCheckError] = useState<string | null>(null)
  const [launching, setLaunching] = useState(false)
  const [launchStatus, setLaunchStatus] = useState('')
  const [launchError, setLaunchError] = useState<string | null>(null)
  const [launchProgress, setLaunchProgress] = useState<number | null>(null)
  const [roomMessage, setRoomMessage] = useState('')
  const [detailsView, setDetailsView] = useState<'rooms' | 'briefing' | 'modpack'>('rooms')
  const handledRoom = useRef<string | null>(null)
  const [pendingRoom, setPendingRoom] = useState<string | null>(null)
  const [serverStatus, setServerStatus] = useState<ServerStatusData | null>(null)

  const handleDeploy = useCallback(async () => {
    // Don't allow deploy until version check completes
    if (!versionInfo || checking || launching || gameBusy || profileChanging || profileBlocked)
      return
    // Update needed OR Java not ready OR Fabric not installed → go to update screen
    if (versionInfo.needsUpdate || !versionInfo.javaOk || !versionInfo.loaderOk) {
      onPlay()
      return
    }
    // Confirmed up to date AND Java OK AND Fabric installed → launch
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
          } else {
            setLaunchProgress(null)
          }
          if (p.filePath) setLaunchStatus(p.filePath)
        })
        await launchGame()
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
  }, [versionInfo, checking, launching, gameBusy, profileChanging, profileBlocked, onPlay, t])

  const handleJoin = useCallback(
    async (id: string) => {
      try {
        if (game.phase === 'running') {
          await invoke('join_running_room', { id })
          await invoke('select_room', { id: null })
          setRoomMessage('Запрос передан игре. Результат входа появится в игровом меню.')
        } else {
          await invoke('select_room', { id })
          setRoomMessage(
            id === 'lobby'
              ? 'После подготовки игра подключится к лобби.'
              : 'После подготовки игра подключится к выбранному матчу.',
          )
          await handleDeploy()
        }
      } catch (error) {
        setRoomMessage(`Не удалось принять приглашение. ${String(error)}`)
      }
    },
    [game.phase, handleDeploy],
  )

  useEffect(() => {
    if (!isTauri()) return
    let active = true
    const refresh = () =>
      invoke<string | null>('pending_room').then((id) => {
        if (active) setPendingRoom(id)
      })
    const subscription = listen('room://pending', () => {
      handledRoom.current = null
      void refresh()
    })
    void subscription.then(refresh).catch(console.error)
    return () => {
      active = false
      void subscription.then((stop) => stop())
    }
  }, [])

  useEffect(() => {
    if (
      !pendingRoom ||
      profile !== 'game' ||
      // Left unhandled so the invite is joined once maintenance ends.
      (pendingRoom !== 'lobby' && serverStatus?.maintenance) ||
      (game.phase !== 'running' && (checking || !versionInfo)) ||
      launching ||
      (gameBusy && game.phase !== 'running') ||
      handledRoom.current === pendingRoom
    )
      return
    handledRoom.current = pendingRoom
    void handleJoin(pendingRoom)
  }, [
    pendingRoom,
    profile,
    serverStatus?.maintenance,
    checking,
    versionInfo,
    launching,
    gameBusy,
    game.phase,
    handleJoin,
  ])

  const checkVersion = useCallback(async (verify = false) => {
    if (!isTauri()) return
    setChecking(true)
    setCheckError(null)
    setLaunchError(null)
    try {
      // Result lands in the shared store (src/lib/modpack-check.ts); versionInfo above re-renders.
      await checkModpack(verify)
    } catch (e) {
      console.error('Failed to check modpack version:', e)
      setCheckError(String(e))
    } finally {
      setChecking(false)
    }
  }, [])

  // Check version on mount (if running in Tauri). The periodic re-check on a timer/focus that
  // keeps the shared store fresh regardless of the active tab lives in App.tsx.
  useEffect(() => {
    if (!isTauri()) return
    // eslint-disable-next-line react-hooks/set-state-in-effect -- intentional sync: this is an external system subscription (Tauri IPC)
    checkVersion()
  }, [checkVersion])

  useEffect(() => {
    let active = true
    let refreshing = false
    const refresh = async () => {
      if (refreshing) return
      refreshing = true
      try {
        const status = await fetchServerStatus()
        if (active) setServerStatus(status)
      } catch {
        if (active) setServerStatus(null)
      } finally {
        refreshing = false
      }
    }
    void refresh()
    const interval = window.setInterval(() => {
      void refresh()
    }, 10_000)
    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [])

  // Derive display values from version check result
  const isChecked = versionInfo !== null
  const needsUpdate = versionInfo?.needsUpdate ?? false
  const needsSetup = isChecked && (needsUpdate || !versionInfo.javaOk || !versionInfo.loaderOk)
  const upToDate = isChecked && !needsSetup
  const installedVersion = versionInfo?.installedVersion ?? '...'
  const latestVersion = versionInfo?.remoteVersion ?? '...'
  const totalSize = versionInfo?.totalSize ? `${(versionInfo.totalSize / 1e9).toFixed(1)} GB` : '—'
  const launchProgressText = launchProgress === null ? '' : ` · ${launchProgress.toFixed(0)}%`
  const launchSub =
    game.phase === 'running'
      ? 'Игра запущена'
      : game.phase === 'finishing'
        ? 'Команды после выхода'
        : launching
          ? `${launchStatus || t('main.launching')}${launchProgressText}`.slice(0, 64)
          : launchError
            ? 'Подробности ниже'
            : !isChecked
              ? t('main.checking')
              : needsSetup
                ? t(needsUpdate ? 'main.updateAvailable' : 'main.setupRequired')
                : t('main.enterBattlefield')
  const launchLabel = gameBusy
    ? gameStateLabel(game.phase)
    : launching
      ? t('main.launching')
      : launchError
        ? 'ПОВТОРИТЬ'
        : !isChecked
          ? '...'
          : needsSetup
            ? t(needsUpdate ? 'main.updateAction' : 'main.prepare')
            : t('update.playNow')

  // Modpack status line
  const modpackStatus: { value: string; tone: Tone } = checking
    ? { value: t('main.checking'), tone: 'muted' }
    : checkError
      ? { value: 'НЕ ПРОВЕРЕН', tone: 'warn' }
      : upToDate
        ? { value: t('main.upToDate'), tone: 'ok' }
        : needsSetup
          ? { value: t(needsUpdate ? 'main.updateAvailable' : 'main.setupRequired'), tone: 'warn' }
          : { value: t('main.upToDate'), tone: 'muted' }
  const operationName =
    profile === 'workshop'
      ? 'Мастерская карт'
      : (localizedContentText(
          content,
          lang,
          'content.operationName',
          'operationName',
          'operation_name',
        ) ?? OPERATION_NAME)
  const season = localizedContentText(content, lang, 'content.season', 'season') ?? t('main.season')
  const description =
    profile === 'workshop'
      ? 'Строительство карт с отдельными настройками и папкой игры.'
      : (localizedContentText(content, lang, 'content.description', 'description') ??
        t('main.description'))
  const serverName =
    profile === 'workshop'
      ? 'Мастерская'
      : (localizedContentText(content, lang, 'content.serverName', 'serverName', 'server_name') ??
        t('main.serverName'))
  const serverIp =
    profile === 'workshop'
      ? 'Адрес в настройках профиля'
      : (contentText(content, 'serverIp', 'server_ip') ?? SERVER_IP)
  const gameUpdating = serverStatus?.online === true && serverStatus.gameAvailable === false
  const maintenance = serverStatus?.online ? (serverStatus.maintenance?.message ?? null) : null
  const players = serverStatus?.playersOnline?.toString() ?? '—'
  const ping = serverStatus?.serverLatencyMs?.toString() ?? '—'
  const region = serverStatus?.regionCode?.trim() || '—'
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

  return (
    <div className="main-screen relative h-full w-full overflow-hidden bg-[#070604]">
      <TopoBackdrop />
      <GridBackdrop intensity={0.5} />

      <div className="screen-layout relative h-full min-h-0">
        <section className="main-layout relative flex h-full flex-col min-h-0">
          <OperationBar label={t('main.operation')} />

          <div className="main-summary flex flex-col md:flex-row min-w-0 items-start justify-between gap-6">
            <div className="max-w-[660px]">
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-1.5">
                <h1 className="tracking-[0.04em] text-[34px] leading-tight text-neutral-50">
                  {operationName}
                </h1>
                <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">{season}</span>
              </div>
              <p className="text-[13px] leading-relaxed text-[#C7AE86] max-w-[620px]">
                {description}
              </p>
            </div>

            <ServerStatus
              serverName={serverName}
              serverIp={serverStatus ? `${serverStatus.host}:${serverStatus.port}` : serverIp}
              online={serverStatus?.online === true}
              updating={gameUpdating}
              maintenance={maintenance}
            />
          </div>

          {/* PLAY zone */}
          <GlowPanel className="main-play p-4 md:p-5">
            {onSelectProfile && (
              <div className="mb-4 flex flex-wrap items-center gap-3">
                <label htmlFor="client-profile" className="text-[12px] text-[#C7AE86]">
                  Профиль
                </label>
                <ProfileSelect
                  value={profile}
                  disabled={profileChanging || checking || launching || gameBusy}
                  onChange={onSelectProfile}
                />
                <span role="status" className="text-[12px] text-[#C7AE86]">
                  {profileChanging
                    ? 'Смена профиля…'
                    : profileBlocked
                      ? 'Ожидание смены профиля…'
                      : profile === 'workshop'
                        ? 'Свои игровые настройки и файлы. Аккаунт общий.'
                        : 'Основная игровая установка.'}
                </span>
              </div>
            )}
            {profileError && (
              <div role="alert" className="mb-4 text-[12px] text-[#c98b8b]">
                {profileError}
              </div>
            )}
            <div className="flex flex-wrap items-center justify-between gap-5">
              <div className="flex flex-wrap min-w-0 items-center gap-4">
                <DeployButton
                  onPlay={handleDeploy}
                  disabled={
                    !isChecked ||
                    checking ||
                    launching ||
                    gameBusy ||
                    profileChanging ||
                    profileBlocked
                  }
                  label={launchLabel}
                  sub={launchSub}
                  busy={launching}
                  inGame={game.phase === 'running'}
                />
                <div className="flex flex-col gap-2 pl-2">
                  <Stat
                    label={t('main.modpack')}
                    value={modpackStatus.value}
                    tone={modpackStatus.tone}
                  />
                  <Stat label={t('main.auth')} value={t('main.verified')} tone="ok" />
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
                  sub=""
                  title={serverStatus?.locationName || t('main.region')}
                />
              </div>
            </div>
          </GlowPanel>

          {(roomMessage || (profile === 'workshop' && pendingRoom)) && (
            <div
              role="status"
              className="mt-2 flex shrink-0 items-start gap-3 text-[12px] text-[#C7AE86]"
            >
              <p className="min-w-0 flex-1 break-words">
                {roomMessage || 'Есть приглашение в игру. Для входа выберите профиль «Игра».'}
              </p>
              {roomMessage && (
                <button
                  type="button"
                  onClick={() => setRoomMessage('')}
                  className="shrink-0 underline underline-offset-4 focus-visible:outline-2 focus-visible:outline-[#F5A524]"
                >
                  Закрыть
                </button>
              )}
            </div>
          )}

          <div
            data-view={detailsView}
            className="main-details mt-4 grid grid-cols-1 md:grid-cols-[minmax(0,1.6fr)_minmax(300px,1fr)] gap-px bg-[#18130D] border border-[#2A2116] flex-1 min-h-0"
          >
            <div className="bg-[#0B0906] p-4 min-h-0 flex flex-col">
              <div
                aria-label={
                  profile === 'workshop' ? 'Информация о мастерской' : 'Информация об игре'
                }
                className="mb-2 flex shrink-0 items-center gap-4 border-b border-[#2A2116]"
              >
                {(
                  [
                    ['rooms', profile === 'workshop' ? 'Мастерская' : 'Матчи'],
                    ['briefing', 'Брифинг'],
                    ['modpack', 'Сборка'],
                  ] as const
                ).map(([view, label]) => (
                  <button
                    key={view}
                    type="button"
                    aria-pressed={detailsView === view}
                    onClick={() => setDetailsView(view)}
                    className={`h-8 border-b text-[12px] transition-colors focus-visible:outline-2 focus-visible:outline-[#F5A524] ${view === 'modpack' ? 'md:hidden' : ''} ${detailsView === view ? 'border-[#F5A524] text-[#F3E7D0]' : 'border-transparent text-[#8E7A5E] hover:text-[#F3E7D0]'}`}
                  >
                    {label}
                    {view === 'rooms' && serverStatus?.rooms
                      ? ` · ${serverStatus.rooms.length}`
                      : ''}
                  </button>
                ))}
              </div>
              <div hidden={detailsView !== 'rooms'} className="min-h-0 flex-1 flex flex-col">
                {profile === 'workshop' ? (
                  <p className="text-[13px] leading-relaxed text-[#C7AE86]">
                    Вход доступен участникам мастерской во время открытой сессии. Адрес подключения
                    задаётся в настройках этого профиля.
                  </p>
                ) : (
                  <RoomList
                    rooms={serverStatus?.rooms}
                    updating={gameUpdating}
                    maintenance={maintenance}
                    onJoin={(id) => {
                      void handleJoin(id)
                    }}
                    disabled={
                      launching ||
                      (game.phase !== 'running' && (checking || !versionInfo || gameBusy))
                    }
                  />
                )}
              </div>
              <div
                hidden={detailsView === 'rooms'}
                className={`min-h-0 flex-1 ${detailsView === 'modpack' ? 'hidden md:block' : ''}`}
              >
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-5">
                  {displayFeatures.map((f) => (
                    <FeatureItem key={f.title} icon={f.icon} title={f.title} desc={f.desc} />
                  ))}
                </div>
              </div>
            </div>

            <div
              className={`main-modpack bg-[#0B0906] p-4 md:p-5 flex-col min-h-0 ${detailsView === 'modpack' ? 'flex' : 'hidden md:flex'}`}
            >
              <SectionHeader label={t('main.modpackStatus')} code="PKG-LIVE" />
              <div className="mt-3 flex flex-col gap-2 flex-1">
                <Row label={t('main.installed')} value={installedVersion} />
                <Row label={t('main.latest')} value={latestVersion} highlight={needsUpdate} />
                <Row label={t('main.size')} value={totalSize} />
                <Row
                  label={t('main.autoUpdate')}
                  value={
                    checking
                      ? t('main.checking')
                      : checkError
                        ? 'НЕ ПРОВЕРЕН'
                        : needsSetup
                          ? t(needsUpdate ? 'main.updateAvailable' : 'main.setupRequired')
                          : t('main.upToDate')
                  }
                  highlight={needsSetup}
                />
                {checkError && (
                  <ErrorDetail
                    message={friendlyError(checkError, 'modpack-check').message}
                    raw={checkError}
                  />
                )}
                {launchError && (
                  <ErrorDetail
                    message={friendlyError(launchError, 'launch').message}
                    raw={launchError}
                  />
                )}
              </div>
              <button
                onClick={() => checkVersion(true)}
                disabled={checking || gameBusy}
                className="mt-3 h-8 border border-[#2A2116] hover:border-[#8A571C] text-[10px] tracking-[0.16em] text-[#C7AE86] hover:text-[#F3E7D0] flex items-center justify-center gap-2 transition-colors disabled:opacity-50"
              >
                {checking ? (
                  <LoaderCircle size={12} className="animate-spin" />
                ) : (
                  <RefreshCw size={12} />
                )}
                {checking ? t('main.checking') : t('main.verifyFiles')}
              </button>
            </div>
          </div>
        </section>
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
  inGame,
}: {
  onPlay: () => void
  label: string
  sub: string
  disabled?: boolean
  busy?: boolean
  inGame?: boolean
}) {
  return (
    <button
      onClick={onPlay}
      disabled={disabled}
      className={`group relative h-[64px] w-[230px] shrink-0 overflow-hidden border transition-all ${
        inGame
          ? 'border-[#82D66B]/40 bg-[#0B0906] cursor-default'
          : disabled
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
        {inGame ? (
          <Gamepad2 size={18} className="text-[#82D66B]" />
        ) : busy ? (
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
  updating,
  maintenance,
}: {
  serverName: string
  serverIp: string
  online: boolean
  updating: boolean
  maintenance: string | null
}) {
  const { t } = useI18n()
  return (
    <div className="shrink-0 border border-[#2A2116] bg-[#0B0906] px-4 py-3 w-full md:w-[340px] max-w-full">
      <div className="flex items-start justify-between gap-2">
        <span className="text-[10px] tracking-[0.14em] text-[#8E7A5E]">{t('main.server')}</span>
        <span
          title={
            maintenance != null
              ? maintenance || GAME_MAINTENANCE
              : updating
                ? GAME_UPDATING
                : undefined
          }
          className={`flex items-center gap-1.5 text-[10px] tracking-[0.16em] ${updating ? 'text-[#F5A524]' : online ? 'text-[#82D66B]' : 'text-[#E36A5D]'}`}
        >
          <StatusDot pulse={online} color={updating ? '#F5A524' : online ? '#82D66B' : '#E36A5D'} />
          {maintenance != null
            ? 'ТЕХРАБОТЫ'
            : updating
              ? 'ОБНОВЛЯЕТСЯ'
              : online
                ? t('main.online')
                : 'НЕДОСТУПЕН'}
        </span>
      </div>
      <div className="mt-2 flex items-center gap-2">
        <ShieldCheck size={14} className="shrink-0 text-[#F5A524]" />
        <span
          title={serverName}
          className="min-w-0 truncate tracking-[0.18em] text-[13px] text-neutral-100"
        >
          {serverName}
        </span>
      </div>
      <div title={serverIp} className="mt-2 text-[10px] tracking-[0.08em] text-[#8E7A5E] truncate">
        {serverIp}
      </div>
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
  title,
}: {
  icon: ReactNode
  label: string
  value: string
  sub: string
  title?: string
}) {
  return (
    <div title={title} className="flex flex-col items-end gap-1 min-w-[58px]">
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
      <div className="mt-0.5 size-7 shrink-0 grid place-items-center border border-[#2A2116] bg-[#11100D] text-[#F5A524]">
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

function featureIcon(icon?: string) {
  return (icon && FEATURE_ICONS[icon]) || <Flag size={14} />
}
