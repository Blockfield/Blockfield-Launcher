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

const CONTEXT_FALLBACK: Record<ErrorContext, string> = {
  'modpack-check': 'Не удалось проверить обновление модпака.',
  'modpack-download': 'Не удалось скачать обновление модпака.',
  launch: 'Не удалось запустить игру.',
  'launcher-update': 'Не удалось проверить или установить обновление лаунчера.',
  'settings-save': 'Не удалось сохранить настройки. Проверьте доступ к папке настроек и повторите.',
  'settings-load': 'Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите.',
}

const CONTEXT_NETWORK: Record<ErrorContext, string> = {
  'modpack-check':
    'Нет доступа к серверу сборки modpack.nether.pp.ua — проверьте интернет и повторите.',
  'modpack-download':
    'Нет доступа к серверу сборки modpack.nether.pp.ua — проверьте интернет и повторите.',
  launch: 'Нет доступа к серверу сборки modpack.nether.pp.ua — проверьте интернет и повторите.',
  'launcher-update': 'Нет доступа к серверу обновлений лаунчера — проверьте интернет и повторите.',
  'settings-save': 'Не удалось сохранить настройки. Проверьте доступ к папке настроек и повторите.',
  'settings-load': 'Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите.',
}

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
  if (looksLikeNetworkError(text)) {
    return { message: CONTEXT_NETWORK[context], raw: text }
  }
  return { message: CONTEXT_FALLBACK[context], raw: text }
}
