import { describe, expect, it } from 'vitest'
import { makeT } from '../i18n'
import { localizedContentText } from '../../lib/content'

describe('makeT', () => {
  it('formats translated placeholders', () => {
    expect(makeT('en')('shell.launcherVersion', { v: '1.2.3' })).toBe('LAUNCHER v1.2.3')
  })

  it('keeps launcher UI translations in the bundled dictionary', () => {
    expect(makeT('ru')('main.tag.patch')).toBe('ПАТЧ')
  })

  it('uses localized CMS content with a per-key English fallback', () => {
    const content = {
      operation_name: 'IRON FRONT',
      translations: {
        en: { 'content.description': 'English CMS description' },
        ru: { 'content.operationName': 'ЖЕЛЕЗНЫЙ ФРОНТ' },
        uk: {},
      },
    }

    expect(localizedContentText(content, 'ru', 'content.operationName', 'operation_name')).toBe(
      'ЖЕЛЕЗНЫЙ ФРОНТ',
    )
    expect(localizedContentText(content, 'uk', 'content.operationName', 'operation_name')).toBe(
      'IRON FRONT',
    )
    expect(localizedContentText(content, 'uk', 'content.description')).toBe(
      'English CMS description',
    )
  })
})
