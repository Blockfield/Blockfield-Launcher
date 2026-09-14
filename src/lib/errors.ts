/**
 * Turns a raw error (usually `String(e)` from a Tauri `invoke` rejection, which is
 * whatever Rust's `format!`/reqwest produced) into a short Russian message that says
 * what failed and what to do. The raw text stays available for a spoiler/details view.
 */
export type ErrorContext =
  | 'modpack-check'
  | 'modpack-download'
  | 'launch'
  | 'launcher-update'
  | 'settings-save'
  | 'settings-load'
  | 'skin-upload'
  | 'account'

const CONTEXT_FALLBACK: Record<ErrorContext, string> = {
  'modpack-check': 'Не удалось проверить обновление модпака.',
  'modpack-download': 'Не удалось скачать обновление модпака.',
  launch: 'Не удалось запустить игру.',
  'launcher-update': 'Не удалось проверить или установить обновление лаунчера.',
  'settings-save': 'Не удалось сохранить настройки. Откройте подробности ошибки.',
  'settings-load': 'Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите.',
  'skin-upload': 'Не удалось загрузить скин. Откройте подробности ошибки.',
  account: 'Не удалось выполнить запрос к серверу аккаунтов.',
}

const CONTEXT_NETWORK: Record<ErrorContext, string> = {
  'modpack-check':
    'Нет доступа к серверу сборки blockfield.pro — проверьте интернет и повторите.',
  'modpack-download':
    'Нет доступа к серверу сборки blockfield.pro — проверьте интернет и повторите.',
  launch: 'Нет доступа к серверу сборки blockfield.pro — проверьте интернет и повторите.',
  'launcher-update': 'Нет доступа к серверу обновлений лаунчера — проверьте интернет и повторите.',
  'settings-save': 'Не удалось сохранить настройки. Откройте подробности ошибки.',
  'settings-load': 'Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите.',
  'skin-upload':
    'Нет доступа к серверу скинов skins.blockfield.pro — проверьте интернет и повторите.',
  account: 'Нет доступа к серверу аккаунтов skins.blockfield.pro — проверьте интернет и повторите.',
}

export const SESSION_EXPIRED = 'SESSION_EXPIRED'

const DRASL_ERRORS: [string, string][] = [
  ['Invalid credentials', 'Неверный никнейм или пароль.'],
  ['That username is taken', 'Этот никнейм уже занят.'],
  ['That player name is taken', 'Этот никнейм уже занят.'],
  ['Invalid password', 'Пароль слишком короткий — нужно не меньше 8 символов.'],
]

// Substrings that show up in reqwest/OS network failures surfaced through Rust's
// `format!("...: {e}")` error strings (see src-tauri/src: pack::fetch_*, download::*, updater.rs).
const NETWORK_HINTS = [
  'error trying to connect',
  'error sending request',
  'dns error',
  'connection refused',
  'timed out',
  'network is unreachable',
  'tcp connect',
]

function looksLikeNetworkError(raw: string): boolean {
  const lower = raw.toLowerCase()
  return NETWORK_HINTS.some((hint) => lower.includes(hint))
}

export interface FriendlyError {
  /** Short, non-misleading message shown to the user. */
  message: string
  /** Raw error text for a collapsed "details" view. */
  raw: string
}

export function friendlyError(raw: unknown, context: ErrorContext): FriendlyError {
  const text = String(raw)
  if (text.includes(SESSION_EXPIRED)) {
    return { message: 'Сессия истекла — войдите снова.', raw: text }
  }
  if (text.includes('Drasl account server unavailable')) {
    return { message: 'Сервер аккаунтов недоступен, попробуйте позже.', raw: text }
  }
  const drasl = text.match(/Drasl: (.*)/)?.[1]
  if (drasl && context === 'account') {
    const known = DRASL_ERRORS.find(([prefix]) => drasl.startsWith(prefix))
    return { message: known ? known[1] : drasl, raw: text }
  }
  if (context === 'settings-save') {
    const settingsErrors: [string, string][] = [
      [
        'Game directory must be a safe absolute path',
        'Выберите отдельную папку для игры, например D:\\Blockfield, а не корень диска.',
      ],
      [
        'Game directory cannot be created:',
        'Не удалось создать папку игры. Выберите другую папку или проверьте доступ к выбранному пути.',
      ],
      [
        'Game directory is not writable:',
        'Не удалось записать файл в папку игры. Выберите другую папку или проверьте права на запись.',
      ],
      [
        'Selected Java executable does not exist',
        'Выбранный файл Java не найден. Очистите поле для автоматического выбора или укажите существующий файл.',
      ],
      [
        'Failed to run selected Java:',
        'Не удалось запустить выбранную Java. Очистите поле для автоматического выбора или выберите другую Java.',
      ],
      [
        'Selected Java must be a working Java 17 or 21 runtime',
        'Для игры нужна Java 17 или 21. Очистите поле для автоматического выбора или выберите подходящую Java.',
      ],
      [
        'RAM must be between',
        'Выбранный объём RAM недопустим. Уменьшите его с учётом памяти компьютера; минимум — 2 ГБ.',
      ],
    ]
    const match = settingsErrors.find(([prefix]) => text.startsWith(prefix))
    if (match) return { message: match[1], raw: text }
  }
  if (looksLikeNetworkError(text)) {
    return { message: CONTEXT_NETWORK[context], raw: text }
  }
  return { message: CONTEXT_FALLBACK[context], raw: text }
}
