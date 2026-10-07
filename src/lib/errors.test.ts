import { describe, expect, it } from 'vitest'
import { friendlyError } from './errors'

describe('friendlyError', () => {
  it('maps network failures to an actionable message per context', () => {
    const result = friendlyError(
      new Error('error sending request for url: error trying to connect: dns error: failed'),
      'modpack-check',
    )
    expect(result.message).toContain('blockfield.pro')
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

describe('settings validation errors', () => {
  it('keeps profile recovery instructions visible instead of hiding them behind a generic launch failure', () => {
    const message = 'Укажите адрес мастерской в настройках выбранного профиля.'
    expect(friendlyError(new Error(message), 'launch').message).toBe(message)
  })
  it.each([
    ['Game directory must be a safe absolute path, not a filesystem root', 'а не корень диска'],
    ['Game directory cannot be created: Access is denied. (os error 5)', 'создать папку игры'],
    ['Game directory is not writable: Permission denied (os error 13)', 'права на запись'],
    ['Selected Java executable does not exist', 'файл Java не найден'],
    ['Failed to run selected Java: Access is denied. (os error 5)', 'запустить выбранную Java'],
    ['Selected Java must be a working Java 17 or 21 runtime', 'Java 17 или 21'],
    ['RAM must be between 2048 and 6144 MB', 'объём RAM'],
  ])('explains %s without blaming the settings directory', (raw, expected) => {
    const result = friendlyError(raw, 'settings-save')
    expect(result.message).toContain(expected)
    expect(result.message).not.toContain('папке настроек')
    expect(result.raw).toBe(raw)
  })

  it('does not guess the cause of an unknown save failure', () => {
    expect(friendlyError('Unexpected failure', 'settings-save').message).not.toContain('доступ')
  })
})
