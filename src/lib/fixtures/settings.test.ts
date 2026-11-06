import { describe, expect, it } from 'vitest'
import { SETTINGS_CAPTURE_STATES, settingsFailure } from './settings'

describe('capture-only settings failures', () => {
  it('rejects only the requested error operation', () => {
    expect(settingsFailure('settings-general-error', 'change_password')).toBe(
      'Drasl: Invalid password',
    )
    for (const state of ['settings-runtime-error', 'settings-commands-error']) {
      expect(settingsFailure(state, 'save_settings')).toBe(
        'Selected Java executable does not exist',
      )
      expect(settingsFailure(state, 'load_settings')).toBeNull()
    }
    for (const state of SETTINGS_CAPTURE_STATES.filter((state) => state.endsWith('-edited'))) {
      expect(settingsFailure(state, 'save_settings')).toBeNull()
      expect(settingsFailure(state, 'change_password')).toBeNull()
    }
    expect(settingsFailure('settings-runtime', 'save_settings')).toBeNull()
  })
})
