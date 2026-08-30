export interface AuthUser {
  id: number
  username: string
  email?: string | null
  minecraft_uuid: string
  role: string
}

export interface AuthSession {
  accessToken: string
  refreshToken: string
  expiresIn: number
  user: AuthUser
  remember: boolean
}

const KEY = 'blockfield.auth'
const API = apiBaseUrl()

export async function login(username: string, password: string, remember: boolean) {
  const response = await fetch(`${API}/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password, remember, deviceName: navigator.userAgent }),
  })
  if (!response.ok)
    throw new Error(
      response.status === 401 ? 'Invalid credentials.' : 'Authentication service unavailable.',
    )
  const session = { ...(await response.json()), remember } as AuthSession
  saveSession(session)
  return session
}

export async function register(
  username: string,
  email: string,
  password: string,
  passwordConfirmation: string,
  remember: boolean,
) {
  const response = await fetch(`${API}/auth/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username,
      email,
      password,
      password_confirmation: passwordConfirmation,
      remember,
      deviceName: navigator.userAgent,
    }),
  })
  if (!response.ok)
    throw new Error(
      response.status === 422 ? 'Registration failed.' : 'Authentication service unavailable.',
    )
  const session = { ...(await response.json()), remember } as AuthSession
  saveSession(session)
  return session
}

export async function restoreSession(): Promise<AuthSession | null> {
  const session = loadSession()
  if (!session) return null
  if (await isValid(session.accessToken)) return session

  const response = await fetch(`${API}/auth/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refreshToken: session.refreshToken }),
  })
  if (!response.ok) {
    clearSession()
    return null
  }
  const refreshed = { ...(await response.json()), remember: session.remember } as AuthSession
  saveSession(refreshed)
  return refreshed
}

export async function logout(session: AuthSession | null) {
  clearSession()
  if (!session) return
  await fetch(`${API}/auth/logout`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${session.accessToken}` },
  }).catch(() => undefined)
}

function loadSession(): AuthSession | null {
  for (const storage of [localStorage, sessionStorage]) {
    try {
      const value = storage.getItem(KEY)
      if (value) return JSON.parse(value) as AuthSession
    } catch {
      clearSession()
    }
  }
  return null
}

function saveSession(session: AuthSession) {
  clearSession()
  ;(session.remember ? localStorage : sessionStorage).setItem(KEY, JSON.stringify(session))
}

function clearSession() {
  localStorage.removeItem(KEY)
  sessionStorage.removeItem(KEY)
}

async function isValid(accessToken: string) {
  const response = await fetch(`${API}/auth/me`, {
    headers: { Authorization: `Bearer ${accessToken}` },
  }).catch(() => null)
  return response?.ok === true
}
import { apiBaseUrl } from './api-base'
