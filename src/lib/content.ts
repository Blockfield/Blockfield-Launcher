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
  const english = translations?.en
  const localized = translations?.[lang]
  if (typeof english !== 'object' && typeof localized !== 'object') return undefined
  return {
    ...(typeof english === 'object' ? english : {}),
    ...(typeof localized === 'object' ? localized : {}),
  }
}

export function localizedContentText(
  content: LauncherContent | null,
  lang: string,
  translationKey: string,
  ...keys: string[]
) {
  const translated = contentTranslations(content, lang)?.[translationKey]
  return typeof translated === 'string' && translated.trim()
    ? translated
    : contentText(content, ...keys)
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
