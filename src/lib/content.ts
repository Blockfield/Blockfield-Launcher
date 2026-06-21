import { useEffect, useState } from 'react'
import type { LauncherContent } from './api'

const API_BASE =
  (import.meta.env.VITE_BLOCKFIELD_API_URL as string | undefined) ??
  'http://localhost:3000/api/launcher/v1'

let cached: LauncherContent | null = null
let pending: Promise<LauncherContent | null> | null = null

export function useLauncherContent() {
  const [content, setContent] = useState<LauncherContent | null>(cached)

  useEffect(() => {
    let alive = true
    loadLauncherContent().then((next) => {
      if (alive) setContent(next)
    })
    return () => {
      alive = false
    }
  }, [])

  return content
}

export function contentText(content: LauncherContent | null, ...keys: string[]) {
  for (const key of keys) {
    const value = content?.[key]
    if (typeof value === 'string' && value.trim()) return value
  }
}

export function contentTranslations(content: LauncherContent | null, lang: string) {
  const translations = content?.translations
  const byLang = translations?.[lang] ?? translations?.en
  if (!byLang || typeof byLang !== 'object') return undefined
  return byLang
}

function loadLauncherContent() {
  if (cached) return Promise.resolve(cached)
  pending ??= fetch(`${API_BASE}/content.json`)
    .then((response) => (response.ok ? response.json() : null))
    .then((data: LauncherContent | null) => {
      cached = data
      return data
    })
    .catch(() => null)
  return pending
}
