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
  readonly VITE_BLOCKFIELD_API_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
