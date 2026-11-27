import { describe, expect, it } from 'vitest'
import { translate } from '../i18n'
import { localizedContentText } from '../../lib/content'

describe('translate', () => {
  it('formats placeholders from the Russian dictionary', () => {
    expect(translate('shell.launcherVersion', { v: '1.2.3' })).toBe('ЛАУНЧЕР v1.2.3')
    expect(translate('main.tag.patch')).toBe('ПАТЧ')
  })

  it('uses localized CMS content with a per-key fallback', () => {
    const content = {
      operation_name: 'IRON FRONT',
      translations: { ru: { 'content.operationName': 'ЖЕЛЕЗНЫЙ ФРОНТ' } },
    }
    expect(localizedContentText(content, 'ru', 'content.operationName', 'operation_name')).toBe(
      'ЖЕЛЕЗНЫЙ ФРОНТ',
    )
    expect(localizedContentText(content, 'ru', 'content.description')).toBeUndefined()
  })
})
