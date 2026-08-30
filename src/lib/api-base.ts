export function apiBaseUrl() {
  const configured = import.meta.env.VITE_BLOCKFIELD_PACK_URL?.replace(/\/$/, '')
  if (configured) return configured
  if (import.meta.env.DEV) return 'https://modpack.dev.nether.pp.ua'
  throw new Error('VITE_BLOCKFIELD_PACK_URL is required for production')
}
