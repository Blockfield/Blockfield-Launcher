import { describe, expect, it } from 'vitest'
import { shouldAutoLaunchAfterSync } from './auto-launch'

describe('shouldAutoLaunchAfterSync', () => {
  const ready = { phase: 'complete', playTriggered: true, launching: false, gameBusy: false }
  it('continues a Play-triggered update into launch after install', () => {
    expect(shouldAutoLaunchAfterSync(ready)).toBe(true)
  })
  it('launches when the Play-triggered sync finds nothing to do', () => {
    expect(shouldAutoLaunchAfterSync({ ...ready, phase: 'uptodate' })).toBe(true)
  })
  it('leaves a manual update on the done screen', () => {
    expect(shouldAutoLaunchAfterSync({ ...ready, playTriggered: false })).toBe(false)
  })
  it('does not fire mid-sync or on error', () => {
    for (const phase of ['checking', 'downloading', 'verifying', 'launching', 'error']) {
      expect(shouldAutoLaunchAfterSync({ ...ready, phase })).toBe(false)
    }
  })
  it('does not double-launch while busy', () => {
    expect(shouldAutoLaunchAfterSync({ ...ready, launching: true })).toBe(false)
    expect(shouldAutoLaunchAfterSync({ ...ready, gameBusy: true })).toBe(false)
  })
})
