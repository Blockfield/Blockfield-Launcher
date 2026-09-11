import { describe, expect, it } from 'vitest'
import { friendlyError } from './errors'

describe('friendlyError', () => {
  it('maps network failures to an actionable message per context', () => {
    const result = friendlyError(
      new Error('error sending request for url: error trying to connect: dns error: failed'),
      'modpack-check',
    )
    expect(result.message).toContain('modpack.nether.pp.ua')
    expect(result.raw).toContain('dns error')
  })

  it('falls back to a context-specific message for non-network errors', () => {
    const result = friendlyError('packwiz-installer task failed: exit code 1', 'modpack-download')
    expect(result.message).toBe('Не удалось скачать обновление модпака.')
    expect(result.raw).toBe('packwiz-installer task failed: exit code 1')
  })

  it('does not mistake local OS errors for network failures', () => {
    const result = friendlyError('Permission denied (os error 13)', 'launch')
    expect(result.message).toBe('Не удалось запустить игру.')
    expect(result.raw).toContain('os error 13')
  })

  it('never invents an error the backend did not report', () => {
    const result = friendlyError(
      'Username must be 3-16 characters: letters, digits or _',
      'settings-save',
    )
    expect(result.message).toContain('Не удалось сохранить настройки.')
    expect(result.raw).toBe('Username must be 3-16 characters: letters, digits or _')
  })
})
