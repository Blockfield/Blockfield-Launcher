interface ImportMetaEnv {
  readonly VITE_CAPTURE_FIXTURES?: string
}

declare module '*.png?inline' {
  const url: string
  export default url
}
