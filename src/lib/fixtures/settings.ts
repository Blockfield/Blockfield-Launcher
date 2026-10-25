/** Capture-only settings interactions; production builds never import fixtures. */
export const SETTINGS_CAPTURE_STATES = [
  'settings-general-edited',
  'settings-general-error',
  'settings-runtime-edited',
  'settings-runtime-error',
  'settings-commands-edited',
  'settings-commands-error',
] as const

export function settingsFailure(state: string, command: string): string | null {
  if (state === 'settings-general-error' && command === 'change_password')
    return 'Drasl: Invalid password'
  if (state.endsWith('-error') && state.startsWith('settings-') && command === 'save_settings')
    return 'Selected Java executable does not exist'
  return null
}

export async function driveSettingsScenario(
  state: string,
  click: (text: string, selector?: string) => boolean,
  until: (check: () => boolean, label: string) => Promise<void>,
) {
  if (!(SETTINGS_CAPTURE_STATES as readonly string[]).includes(state)) return
  const tab = state.split('-')[1]
  const edited = state.endsWith('-edited')
  const input = (label: string, value: string) => {
    const field = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      `[aria-label="${label}"]`,
    )
    if (!field) return false
    const prototype =
      field instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value')!.set!.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
    return true
  }
  if (tab === 'general') {
    if (edited) {
      for (const label of [
        'ПРОВЕРКА ПЕРЕД ЗАПУСКОМ',
        'Показывать активность в Discord',
        'Скрывать лаунчер во время игры',
      ]) {
        const toggle = document.querySelector<HTMLButtonElement>(
          `[role="switch"][aria-label="${label}"]`,
        )
        if (!toggle) throw new Error(`Missing settings toggle: ${label}`)
        toggle.click()
      }
    }
    await until(() => click('СМЕНИТЬ ПАРОЛЬ'), 'password panel')
    if (!edited) {
      await until(() => input('Новый пароль', 'short'), 'new password')
      await new Promise((resolve) => setTimeout(resolve, 50))
      document.querySelector<HTMLFormElement>('.settings-panel form')!.requestSubmit()
      await until(
        () => document.body.textContent!.includes('Пароль слишком короткий'),
        'password error',
      )
    }
  } else if (edited && tab === 'commands') {
    await until(
      () => input('ПЕРЕД ЗАПУСКОМ', 'echo preparing\necho "$INST_DIR"'),
      'pre-launch command',
    )
    await until(() => input('ПОСЛЕ ВЫХОДА ИЗ ИГРЫ', 'echo finished'), 'post-exit command')
  } else {
    if (tab === 'commands') await until(() => click('JAVA И ПАМЯТЬ'), 'runtime tab')
    await until(
      () => input('СРЕДА JAVA', edited ? '/capture/java/bin/java' : '/missing/java'),
      'Java path',
    )
    if (edited) await until(() => click('8G'), 'RAM preset')
    else {
      if (tab === 'commands') await until(() => click('КОМАНДЫ'), 'commands tab')
      await new Promise((resolve) => setTimeout(resolve, 50))
      await until(() => click('СОХРАНИТЬ'), 'save settings')
      await until(
        () => document.body.textContent!.includes('Выбранный файл Java не найден'),
        'settings error',
      )
    }
  }
}
