import { describe, expect, it } from 'vitest'
import { CAPTURE_STATES, fixtureResponse, FIXED_TIME } from './data'

describe('capture fixtures', () => {
  it('covers every named state without a network or account dependency', () => {
    for (const state of CAPTURE_STATES) {
      for (const cmd of [
        'load_settings',
        'account_status',
        'game_status',
        'server_status',
        'check_launcher_update',
        'stats_leaderboard',
        'stats_player',
        'stats_seasons',
      ]) {
        expect(fixtureResponse(state, cmd)).toEqual(fixtureResponse(state, cmd))
      }
    }
    expect(fixtureResponse('first-run', 'load_settings')).toMatchObject({ username: '' })
    expect(fixtureResponse('login', 'account_status')).toMatchObject({ loggedIn: false })
    expect(fixtureResponse('main-launching', 'game_status')).toMatchObject({ phase: 'launching' })
    expect(fixtureResponse('update-42', 'check_modpack_version')).toMatchObject({
      needsUpdate: true,
    })
    expect(fixtureResponse('offline', 'server_status')).toMatchObject({ online: false, rooms: [] })
    expect(fixtureResponse('main', 'server_status')).toMatchObject({ checkedAt: FIXED_TIME })
  })
  it('fails closed on unexpected IPC and supplies the error state', () => {
    expect(() => fixtureResponse('main', 'launch_game')).toThrow('unexpected IPC')
    expect(() => fixtureResponse('error', 'check_modpack_version')).toThrow('manifest unavailable')
  })
})
