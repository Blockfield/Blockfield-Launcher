import type { Plugin } from 'vite'

export function captureIPC(): Plugin {
  return {
    name: 'capture-ipc',
    enforce: 'pre',
    transform(code, id) {
      if (!id.endsWith('/node_modules/@tauri-apps/api/core.js')) return
      // Native Tauri invoke is immutable; intercept only the capture bundle's API module.
      const original = 'return window.__TAURI_INTERNALS__.invoke(cmd, args, options);'
      if (!code.includes(original))
        throw new Error('Tauri invoke changed; update capture IPC transform')
      return code.replace(
        original,
        'return (window.__BLOCKFIELD_CAPTURE_INVOKE__ ?? window.__TAURI_INTERNALS__.invoke)(cmd, args, options);',
      )
    },
  }
}
