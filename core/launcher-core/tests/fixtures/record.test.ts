/**
 * Records JSON fixtures from the real TS implementations for the Rust port
 * (unit u31-core-fmt, base b4583ca). Run explicitly:
 *   vitest run core/launcher-core/tests/fixtures/record.test.ts
 * Output: core/launcher-core/tests/fixtures/*.json (committed).
 *
 * Exported pure functions are imported directly. formatSpeed/formatMirror/
 * formatBytes (UpdateScreen.tsx) and gameStateLabel (game-state.ts) are module-
 * private or pull React/Tauri in, so their bodies are extracted verbatim from
 * src and evaluated — same rounding, same strings. The only mock is the Tauri
 * IPC behind `join` (node:path/posix join has identical normalization).
 * formatTime (StatsScreen, Intl locale + local tz) and timestamp() (clock)
 * are intentionally NOT recorded; see HANDOFF in the unit report.
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path/posix'
import { fileURLToPath } from 'node:url'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/path', async () => {
  const posix = await import('node:path/posix')
  return { join: (...parts: string[]) => Promise.resolve(posix.join(...parts)) }
})

import { friendlyError } from '../../../../../src/lib/errors'
import { selectedGameDirectory } from '../../../../../src/lib/game-directory'
import { clampRamGb, ramLimitGb } from '../../../../../src/lib/ram'
import {
  displayRank,
  formatBattles,
  formatKd,
  formatMmr,
  formatMmrRange,
  formatScore,
} from '../../../../../src/lib/stats'
import { isValidUsername, USERNAME_HINT } from '../../../../../src/lib/username'
import { hasWorkshopAccess, WORKSHOP_ROLES } from '../../../../../src/lib/workshop'

const OUT = dirname(fileURLToPath(import.meta.url))
const BASE = 'b4583ca'

/** Extract a module-private function body verbatim from src and evaluate it. */
function extract(sourceFile: string, name: string, params: string[]): (...args: number[] | string[]) => unknown {
  const src = readFileSync(join(process.cwd(), sourceFile), 'utf8')
  const start = src.indexOf(`function ${name}(`)
  if (start < 0) throw new Error(`function ${name} not found in ${sourceFile}`)
  let depth = 0
  let i = src.indexOf('{', start)
  const bodyStart = i + 1
  for (; i < src.length; i += 1) {
    if (src[i] === '{') depth += 1
    else if (src[i] === '}') {
      depth -= 1
      if (depth === 0) break
    }
  }
  if (depth !== 0) throw new Error(`unbalanced braces for ${name}`)
  const body = src.slice(bodyStart, i)
  if (!body.includes('Math.round') && !body.includes('toFixed') && !body.includes('switch') && !body.includes('URL')) {
    throw new Error(`extracted body for ${name} looks wrong`)
  }
  return new Function(...params, body) as (...args: number[] | string[]) => unknown
}

const BASE_META = { base: BASE }

function write(name: string, suite: string, cases: unknown[]): void {
  writeFileSync(join(OUT, `${name}.json`), `${JSON.stringify({ ...BASE_META, suite, cases }, null, 2)}\n`)
}

/** JSON-safe number: non-finite doubles become strings (JSON has no NaN). */
const num = (v: number): number | string => (Number.isFinite(v) ? v : String(v))

describe('record fixtures', () => {
  it('fmt-update.json', () => {
    const formatSpeed = extract('src/app/components/UpdateScreen.tsx', 'formatSpeed', ['bytesPerSec'])
    const formatMirror = extract('src/app/components/UpdateScreen.tsx', 'formatMirror', ['url'])
    const formatBytes = extract('src/app/components/UpdateScreen.tsx', 'formatBytes', ['bytes'])
    const speeds = [0, 0.4, 1, 999, 1023, 1023.4, 1023.5, 1024, 1280, 1536, 1792, 1792.5, 1048575, 1048576, 1310720, 1572864, 2500000, 104857600, 10737418240, -5, -1280, NaN, Infinity]
    const bytes = [0, 1, 512, 1023, 1023.5, 1024, 1280, 1536, 2560, 1047040, 1048064, 1048832, 1048575, 1048576, 1572864, 10485760, 1610612736, 1099511627776, -1, NaN, Infinity]
    const mirrors = [
      'http://host.com:8080/x',
      'https://example.com/path',
      'http://example.com:80/',
      'https://EXAMPLE.com/AbC',
      'http://[::1]:8080/x',
      'ftp://h:21/f',
      'not a url',
      '',
    ]
    const cases = [
      ...speeds.map((input) => ({ fn: 'format_speed', input: num(input), expected: formatSpeed(input) })),
      ...bytes.map((input) => ({ fn: 'format_bytes', input: num(input), expected: formatBytes(input) })),
      ...mirrors.map((input) => ({ fn: 'format_mirror', input, expected: formatMirror(input) })),
    ]
    for (const c of cases) expect(typeof c.expected).toBe('string')
    write('fmt-update', 'UpdateScreen.tsx:43-64 formatSpeed/formatMirror/formatBytes', cases)
  })

  it('fmt-stats.json', () => {
    const mmrs = [
      null,
      undefined,
      { rating: 1500, rd: 50, ratedCampaigns: 10, provisional: false },
      { rating: 1500.4, rd: 50, ratedCampaigns: 10, provisional: false },
      { rating: 1500.5, rd: 50, ratedCampaigns: 10, provisional: false },
      { rating: 0, rd: 100, ratedCampaigns: 0, provisional: true },
      { rating: 1723.49, rd: 12.35, ratedCampaigns: 3, provisional: true },
    ]
    const cases: unknown[] = [
      ...mmrs.map((input) => ({ fn: 'format_mmr', input, expected: formatMmr(input) })),
      ...mmrs.map((input) => ({ fn: 'format_mmr_range', input, expected: formatMmrRange(input) })),
      ...[
        { kills: 0, deaths: 0 },
        { kills: 5, deaths: 3 },
        { kills: 10, deaths: 0 },
      ].map((input) => ({ fn: 'format_kd', input, expected: formatKd(input) })),
      ...[0, 1, 1.2, 1.234, 1.235, 1.005, 2.675, -1.5, 100, 99.995].map((input) => ({
        fn: 'format_score',
        input,
        expected: formatScore(input),
      })),
      ...[
        { matches: 10, wins: 3 },
        { matches: 0, wins: 0 },
      ].map((input) => ({ fn: 'format_battles', input, expected: formatBattles(input) })),
      ...[
        { row: { rank: 5 }, index: 0, page: 1, limit: 25 },
        { row: { rank: null }, index: 0, page: 1, limit: 25 },
        { row: {}, index: 24, page: 2, limit: 25 },
        { row: { rank: 0 }, index: 3, page: 1, limit: 10 },
      ].map((input) => ({ fn: 'display_rank', input, expected: displayRank(input.row, input.index, input.page, input.limit) })),
    ]
    write('fmt-stats', 'stats.ts:164-180 formatMmr/formatMmrRange/formatKd/formatScore/formatBattles/displayRank', cases)
  })

  it('fmt-game-state.json', () => {
    const gameStateLabel = extract('src/lib/game-state.ts', 'gameStateLabel', ['phase'])
    const phases = ['running', 'launching', 'finishing', 'unknown', 'idle']
    const cases = phases.map((input) => ({ fn: 'game_state_label', input, expected: gameStateLabel(input) }))
    for (const c of cases) expect(typeof c.expected).toBe('string')
    write('fmt-game-state', 'game-state.ts:66 gameStateLabel', cases)
  })

  it('roles.json', () => {
    const inputs = [...WORKSHOP_ROLES, null, undefined, '', 'player', 'unknown', 'Admin', ' builder', 'owner ']
    const cases = inputs.map((input) => ({ fn: 'has_workshop_access', input, expected: hasWorkshopAccess(input) }))
    expect(WORKSHOP_ROLES).toEqual(['builder', 'moderator', 'admin', 'owner'])
    write('roles', 'workshop.ts:1-7 hasWorkshopAccess (+ App.tsx:103 usage)', cases)
  })

  it('errors.json', () => {
    const contexts = [
      'modpack-check',
      'modpack-download',
      'launch',
      'launcher-update',
      'settings-save',
      'settings-load',
      'skin-upload',
      'account',
      'select-profile',
    ] as const
    const nets = [
      'reqwest error trying to connect: dns error',
      'error sending request: connection refused',
      'operation timed out',
      'network is unreachable',
      'tcp connect error: connection refused',
    ]
    const settings = [
      'Game directory must be a safe absolute path: C:\\',
      'Game directory cannot be created: EACCES',
      'Game directory is not writable: EACCES',
      'Selected Java executable does not exist: /usr/bin/java',
      'Failed to run selected Java: exit 1',
      'Selected Java must be a working Java 17 or 21 runtime, got 11',
      'RAM must be between 2048 and 16384 MB',
    ]
    const cases: unknown[] = [
      ...contexts.map((context) => ({
        fn: 'friendly_error',
        input: { raw: 'invoke failed: something broke', context },
        expected: friendlyError('invoke failed: something broke', context).message,
      })),
      ...nets.flatMap((raw) =>
        (['modpack-check', 'launcher-update', 'skin-upload', 'account', 'select-profile'] as const).map((context) => ({
          fn: 'friendly_error',
          input: { raw, context },
          expected: friendlyError(raw, context).message,
        })),
      ),
      { fn: 'friendly_error', input: { raw: 'x SESSION_EXPIRED y', context: 'launch' }, expected: friendlyError('x SESSION_EXPIRED y', 'launch').message },
      { fn: 'friendly_error', input: { raw: 'Drasl account server unavailable', context: 'account' }, expected: friendlyError('Drasl account server unavailable', 'account').message },
      ...['Invalid credentials', 'That username is taken', 'That player name is taken', 'Invalid password', 'Weird new drasl failure'].flatMap((drasl) => [
        {
          fn: 'friendly_error',
          input: { raw: `oops Drasl: ${drasl} tail`, context: 'account' },
          expected: friendlyError(`oops Drasl: ${drasl} tail`, 'account').message,
        },
        {
          fn: 'friendly_error',
          input: { raw: `Drasl: ${drasl}`, context: 'launch' },
          expected: friendlyError(`Drasl: ${drasl}`, 'launch').message,
        },
      ]),
      ...settings.map((raw) => ({
        fn: 'friendly_error',
        input: { raw, context: 'settings-save' },
        expected: friendlyError(raw, 'settings-save').message,
      })),
      {
        fn: 'friendly_error',
        input: { raw: 'settings-save Game directory must be a safe absolute path', context: 'settings-save' },
        expected: friendlyError('settings-save Game directory must be a safe absolute path', 'settings-save').message,
      },
      ...[
        'Укажите адрес мастерской в настройках выбранного профиля.',
        'Закройте игру перед сменой профиля.',
        'Дождитесь завершения текущей операции.',
      ].map((raw) => ({
        fn: 'friendly_error',
        input: { raw: `wrap ${raw} wrap`, context: 'modpack-check' },
        expected: friendlyError(`wrap ${raw} wrap`, 'modpack-check').message,
      })),
      {
        fn: 'friendly_error',
        input: { raw: String(new Error('boom')), context: 'launch' },
        expected: friendlyError(String(new Error('boom')), 'launch').message,
      },
    ]
    // raw passthrough is part of the contract
    for (const c of cases as { input: { raw: string } }[]) {
      const r = friendlyError(c.input.raw, 'launch')
      expect(r.raw).toBe(c.input.raw)
    }
    write('errors', 'errors.ts:1-141 friendlyError (+ SESSION_EXPIRED passthrough, network hints)', cases)
  })

  it('validate.json', async () => {
    const ramInputs: { fn: string; input: { value: number | string; max?: number | string } }[] = [
      ...[32768, 0, 1024, 1536, 2048, 16384, 65536, -100, NaN, Infinity].map((value) => ({
        fn: 'ram_limit_gb',
        input: { value: num(value) },
      })),
      ...[
        { value: 1, max: 16 },
        { value: 2, max: 16 },
        { value: 2.4, max: 16 },
        { value: 2.5, max: 16 },
        { value: 8, max: 16 },
        { value: 20, max: 16 },
        { value: NaN, max: 16 },
        { value: Infinity, max: 8 },
        { value: -Infinity, max: 8 },
        { value: -5, max: 16 },
      ].map(({ value, max }) => ({ fn: 'clamp_ram_gb', input: { value: num(value), max } })),
    ]
    const ramCases = ramInputs.map(({ fn, input }) => ({
      fn,
      input,
      expected:
        fn === 'ram_limit_gb'
          ? num(ramLimitGb(input.value as number))
          : num(clampRamGb(input.value as number, input.max as number)),
    }))
    const nameCases = [
      'abc',
      'ab',
      'abcdefghijklmnop',
      'abcdefghijklmnopq',
      'a b',
      'A_Z-9',
      '  abc  ',
      '',
      '   ',
      'иван',
      'ABC_123_xyz45678',
      '_ab',
    ].map((input) => ({ fn: 'is_valid_username', input, expected: isValidUsername(input) }))
    expect(USERNAME_HINT).toBe('Ник в Minecraft (3-16 букв, цифр или _).')
    const dirCases = await Promise.all(
      ['C:/Games', 'C:/Games/Blockfield', 'C:/Games/blockfield/', '/home/u/.games/', '/x/BLOCKFIELD', '', '/', 'relative/dir', 'C:\\Games\\Blockfield', 'C:\\Games\\Other\\'].map(
        async (input) => ({ fn: 'selected_game_directory', input, expected: await selectedGameDirectory(input) }),
      ),
    )
    write('validate', 'ram.ts + username.ts + game-directory.ts', [...ramCases, ...nameCases, ...dirCases])
  })
})
