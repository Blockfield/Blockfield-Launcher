// CMS fallbacks — override via VITE_* env vars at build time
// ponytail: read from import.meta.env, one env var per constant
export const LAUNCHER_VERSION = import.meta.env.VITE_LAUNCHER_VERSION ?? '0.4.2'
export const SERVER_IP = import.meta.env.VITE_SERVER_IP ?? 'play.blockfield.gg:25565'
export const SERVER_REGION = import.meta.env.VITE_SERVER_REGION ?? 'EU-WEST · 28ms'
export const BRAND = import.meta.env.VITE_BRAND ?? 'BLOCKFIELD'
export const OPERATION_NAME = import.meta.env.VITE_OPERATION_NAME ?? 'IRON FRONT'
export const OPERATOR_HANDLE = import.meta.env.VITE_OPERATOR_HANDLE ?? 'KILO_7'
export const OPERATOR_INITIALS = import.meta.env.VITE_OPERATOR_INITIALS ?? 'K7'
export const COORDINATES = import.meta.env.VITE_COORDINATES ?? 'LAT 47.3829° / LON 19.0402°'
export const COPYRIGHT = import.meta.env.VITE_COPYRIGHT ?? '© 2026 BLOCKFIELD COMMAND'
