// CMS fallbacks — override via VITE_* env vars at build time
// ponytail: read from import.meta.env, one env var per constant
export const SERVER_IP = import.meta.env.VITE_SERVER_IP ?? 'minecraft.play.nether.pp.ua:25565'
export const BRAND = import.meta.env.VITE_BRAND ?? 'BLOCKFIELD'
export const OPERATION_NAME = BRAND
export const COPYRIGHT = import.meta.env.VITE_COPYRIGHT ?? '© 2026 BLOCKFIELD COMMAND'
export const RELEASES_REPO_URL =
  import.meta.env.VITE_RELEASES_REPO_URL ??
  'https://github.com/netherg-io/blockfield-launcher-releases'
