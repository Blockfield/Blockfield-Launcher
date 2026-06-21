import { describe, expect, it } from 'vitest'
import { makeT } from '../i18n'

describe('makeT', () => {
  it('formats translated placeholders', () => {
    expect(makeT('en')('shell.launcherVersion', { v: '1.2.3' })).toBe('LAUNCHER v1.2.3')
  })

  it('uses CMS overrides before bundled translations', () => {
    expect(
      makeT('en', { 'shell.launcherVersion': 'BUILD {v}' })('shell.launcherVersion', {
        v: '9.9.9',
      }),
    ).toBe('BUILD 9.9.9')
  })
})
