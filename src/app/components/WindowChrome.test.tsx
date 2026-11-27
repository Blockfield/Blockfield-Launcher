// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import { WindowChrome } from './WindowChrome'

vi.mock('../../lib/content', () => ({
  useLauncherContent: () => ({}),
  localizedContentText: () => 'CMS TITLE',
}))
vi.mock('../i18n', () => ({ useI18n: () => ({ lang: 'ru' }) }))

it('marks every screen as DEV even when its title comes from the CMS', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<WindowChrome>Login screen</WindowChrome>))
    expect(container.textContent).toContain('CMS TITLE')
    expect(container.textContent?.includes('DEV · ТЕСТОВАЯ СБОРКА')).toBe(__BLOCKFIELD_DEV_BUILD__)
    expect(document.title).toBe(__BLOCKFIELD_DEV_BUILD__ ? 'CMS TITLE DEV' : 'CMS TITLE')
    expect(container.textContent).toContain('Login screen')
  } finally {
    await act(async () => root.unmount())
    container.remove()
    vi.unstubAllGlobals()
  }
})
