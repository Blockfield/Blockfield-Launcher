import { describe, expect, it } from 'vitest'
import { makeT } from '../i18n'
import { localizedContentText } from '../../lib/content'

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

  it('uses localized CMS content with an English fallback', () => {
    const content = {
      operation_name: 'IRON FRONT',
      translations: { ru: { 'content.operationName': 'ЖЕЛЕЗНЫЙ ФРОНТ' } },
    }

    expect(localizedContentText(content, 'ru', 'content.operationName', 'operation_name')).toBe(
      'ЖЕЛЕЗНЫЙ ФРОНТ',
    )
    expect(localizedContentText(content, 'uk', 'content.operationName', 'operation_name')).toBe(
      'IRON FRONT',
    )
  })
})
