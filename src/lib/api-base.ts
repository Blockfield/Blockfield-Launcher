export function apiBaseUrl() {
  const configured = import.meta.env.VITE_BLOCKFIELD_API_URL?.replace(/\/$/, '')
  if (configured) return configured
  if (import.meta.env.DEV) return 'http://localhost:3000/api/launcher/v1'
  throw new Error('VITE_BLOCKFIELD_API_URL is required for production')
}
