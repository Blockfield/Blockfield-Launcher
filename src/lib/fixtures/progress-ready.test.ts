// @vitest-environment jsdom
import { readFileSync } from 'node:fs'
import { expect, it, vi } from 'vitest'

it('adds an invisible readiness witness only after the real UI reports 42.0%', async () => {
  vi.useFakeTimers()
  history.replaceState(null, '', '?state=update-42')
  document.body.innerHTML = '<main>0.0%</main>'
  window.eval(readFileSync('scripts/capture/progress-ready.js', 'utf8'))
  expect(document.querySelector('[data-capture-readiness]')).toBeNull()
  document.querySelector('main')!.textContent = '42.0 %'
  await vi.advanceTimersByTimeAsync(1)
  const witness = document.querySelector<HTMLElement>('[data-capture-readiness]')!
  expect(witness.hidden).toBe(true)
  expect(witness.textContent).toBe('42%')
  expect(document.querySelector('main')!.textContent).toBe('42.0 %')
  expect(vi.getTimerCount()).toBe(0)
  vi.useRealTimers()
  history.replaceState(null, '', '/')
})
