import { readFileSync } from 'node:fs'

export function validateProductionUrl(value) {
  if (!value) throw new Error('VITE_BLOCKFIELD_API_URL is required')
  const url = new URL(value)
  const host = url.hostname.toLowerCase()
  if (url.protocol !== 'https:') throw new Error('Production API must use HTTPS')
  if (
    host === 'localhost' ||
    host.endsWith('.localhost') ||
    host.endsWith('.local') ||
    /^(127\.|10\.|192\.168\.|169\.254\.)/.test(host) ||
    /^172\.(1[6-9]|2\d|3[01])\./.test(host) ||
    host === '::1' ||
    /(^|[.-])(dev|staging|test)([.-]|$)/i.test(host)
  ) throw new Error('Production API host is local, private, or development-only')
  if (url.username || url.password) throw new Error('Production API URL must not contain credentials')
  return url
}

const url = validateProductionUrl(process.env.VITE_BLOCKFIELD_API_URL)
const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.prod.conf.json', import.meta.url), 'utf8'))
const csp = config.app?.security?.csp ?? ''
const connectSources = csp.split(';').find((part) => part.trim().startsWith('connect-src')) ?? ''
const networkOrigins = connectSources.match(/https?:\/\/[^\s]+/g) ?? []
if (!networkOrigins.includes(url.origin) || networkOrigins.some((origin) =>
  origin !== url.origin && origin !== 'http://ipc.localhost')) {
  throw new Error(`Production CSP must contain only the configured API origin: ${url.origin}`)
}
if (!config.plugins?.updater?.endpoints?.every((endpoint) => endpoint.startsWith(`${url.origin}/`))) {
  throw new Error('Production updater endpoint must use the production API origin')
}
console.log(`Production API configuration valid: ${url.origin}`)
