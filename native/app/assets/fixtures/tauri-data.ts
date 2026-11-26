import type { LauncherConfig, ServerStatus, VersionCheckResult } from '../api'

export const CAPTURE_STATES = [
  'first-run',
  'login',
  'main',
  'main-launching',
  'update-42',
  'stats',
  'settings-general',
  'settings-runtime',
  'settings-commands',
  'settings-skin',
  'settings-launcher',
  'offline',
  'error',
] as const
export type CaptureState = (typeof CAPTURE_STATES)[number]
export const FIXED_TIME = Date.UTC(2026, 9, 24, 12)
export const UUID = '00000000-0000-4000-8000-000000000005'
export const config: LauncherConfig = {
  activeProfile: 'game',
  workshopServer: 'workshop.example.test:25566',
  gameDir: '/capture/game',
  javaPath: '',
  maxRamMb: 12288,
  ramMb: 4096,
  autoUpdate: false,
  lang: 'ru',
  username: 'CaptureOperator',
  preLaunchCommand: '',
  postExitCommand: '',
  hideWhilePlaying: false,
  discordPresence: false,
}
export const version: VersionCheckResult = {
  needsUpdate: false,
  remoteVersion: '2.75',
  installedVersion: '2.75',
  mirror: 'https://mirror.example.test/pack.toml',
  fileCount: 100,
  totalSize: 1_000_000_000,
  javaOk: true,
  loaderOk: true,
}
export const server: ServerStatus = {
  online: true,
  gameAvailable: true,
  playersOnline: 24,
  playersMax: 100,
  minecraftVersion: '1.21.1',
  motd: 'BLOCKFIELD',
  host: 'play.blockfield.pro',
  port: 25565,
  regionCode: 'EU',
  locationName: 'Frankfurt',
  serverLatencyMs: 32,
  checkedAt: FIXED_TIME,
  apiLatencyMs: 0,
  rooms: [
    {
      id: 'capture-casual',
      name: 'ALPHA',
      mode: 'room',
      map: 'Citadel',
      phase: 'waiting',
      players: 12,
      joinable: true,
      format: 'casual',
      admission: 'open',
      ready: { ready: 8, required: 12 },
    },
    {
      id: 'capture-ranked',
      name: 'BRAVO',
      mode: 'campaign',
      map: 'Crossroads',
      phase: 'running',
      players: 12,
      joinable: true,
      format: 'ranked',
      admission: 'waiting',
    },
  ],
}
export const players = ['CaptureOperator', 'Alpha', 'Bravo', 'Charlie', 'Delta'].map((name, i) => ({
  uuid: i === 0 ? UUID : `00000000-0000-4000-8000-${String(i).padStart(12, '0')}`,
  name,
  score: 1250 - i * 100,
  kills: 50 - i * 5,
  deaths: 20 + i,
  captures: 10 - i,
  neutralizations: 5,
  pointSeconds: 600,
  playSeconds: 7200,
  suicides: 0,
  teamKills: 0,
  maxKillstreak: 8,
  matches: 12,
  wins: 7,
  kd: 2.5,
  rank: i + 1,
  mmr: { rating: 1500 - i * 20, rd: 70, ratedCampaigns: 12, provisional: false },
}))

export function fixtureResponse(state: CaptureState, command: string): unknown {
  switch (command) {
    case 'load_settings':
      return { ...config, username: state === 'first-run' ? '' : config.username }
    case 'account_status':
      return {
        loggedIn: state !== 'login',
        username: config.username,
        uuid: UUID,
        needsPassword: false,
      }
    case 'game_status':
      return { phase: state === 'main-launching' ? 'launching' : 'idle', revision: 1 }
    case 'check_modpack_version':
      if (state === 'error') throw new Error('Capture fixture: manifest unavailable')
      return {
        ...version,
        needsUpdate: state === 'update-42',
        installedVersion: state === 'update-42' ? '2.74' : version.installedVersion,
      }
    case 'server_status':
      return state === 'offline'
        ? {
            ...server,
            online: false,
            gameAvailable: null,
            rooms: [],
            playersOnline: null,
            serverLatencyMs: null,
          }
        : server
    case 'check_launcher_update':
      return {
        currentVersion: '1.0.19',
        version: '1.0.20',
        body: 'Capture fixture release notes.\nDeterministic update available.',
      }
    case 'stats_leaderboard':
      return { schemaVersion: 1, page: 1, limit: 20, total: players.length, players }
    case 'stats_player':
      return {
        schemaVersion: 1,
        profile: { uuid: UUID, name: config.username, lastSeen: FIXED_TIME, role: 'player' },
        totals: players[0],
        classes: [],
        campaigns: { completed: 12, wins: 7 },
        history: [],
        mmr: players[0]!.mmr,
        ratingHistory: [],
      }
    case 'stats_seasons':
      return {
        current: 'capture-season',
        seasons: [
          {
            id: 'capture-season',
            name: 'SEASON 01',
            startsAt: '2026-10-01T00:00:00Z',
            endsAt: null,
          },
        ],
      }
    case 'pending_room':
    case 'dev_status':
      return null
    case 'verify_files':
      return []
    case 'save_settings':
    case 'select_room':
    case 'cancel_download':
      return null
    case 'plugin:app|version':
      return '1.0.19'
    default:
      throw new Error(`Capture fixture: unexpected IPC ${command}`)
  }
}
