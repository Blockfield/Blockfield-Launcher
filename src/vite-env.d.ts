declare module '*.css' {
  const content: Record<string, string>
  export default content
}

declare module '*.svg' {
  const content: string
  export default content
}

declare module '*.csv' {
  const content: string[][]
  export default content
}

interface ImportMetaEnv {
  readonly DEV: boolean
  readonly VITE_BLOCKFIELD_API_URL?: string
  // Branding / CMS fallbacks
  readonly VITE_LAUNCHER_VERSION?: string
  readonly VITE_SERVER_IP?: string
  readonly VITE_SERVER_REGION?: string
  readonly VITE_BRAND?: string
  readonly VITE_OPERATION_NAME?: string
  readonly VITE_OPERATOR_HANDLE?: string
  readonly VITE_OPERATOR_INITIALS?: string
  readonly VITE_COORDINATES?: string
  readonly VITE_COPYRIGHT?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
