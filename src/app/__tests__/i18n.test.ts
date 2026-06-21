import { describe, expect, it } from 'vitest'
import { makeT } from '../i18n'

describe('makeT', () => {
  it('formats translated placeholders', () => {
    expect(makeT('en')('shell.launcherVersion', { v: '1.2.3' })).toBe('LAUNCHER v1.2.3')
  })
})
