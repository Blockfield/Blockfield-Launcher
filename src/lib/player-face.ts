import { invoke } from '@tauri-apps/api/core'
import type { SkinPreview } from './api'

// mc-heads.net only knows Mojang accounts, so Drasl-only players with a custom skin never showed
// it here; resolve through the same Drasl/Mojang chain as the skin preview and crop the face.
const faceCache = new Map<string, Promise<string | null>>()

// A skin upload must show up without a restart: drop the cached face and re-run mounted heads.
let faceVersion = 0
export const getFaceVersion = () => faceVersion
const faceListeners = new Set<() => void>()
export const subscribeFaces = (listener: () => void) => {
  faceListeners.add(listener)
  return () => faceListeners.delete(listener)
}

export function forgetFace(username: string) {
  faceCache.delete(username)
  faceVersion++
  faceListeners.forEach((listener) => listener())
}

function faceFromSkin(skinDataUrl: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => {
      const canvas = document.createElement('canvas')
      canvas.width = 8
      canvas.height = 8
      const ctx = canvas.getContext('2d')
      if (!ctx) {
        reject(new Error('no 2d context'))
        return
      }
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(image, 8, 8, 8, 8, 0, 0, 8, 8) // base face
      ctx.drawImage(image, 40, 8, 8, 8, 0, 0, 8, 8) // hat overlay
      resolve(canvas.toDataURL('image/png'))
    }
    image.onerror = () => reject(new Error('failed to decode skin'))
    image.src = skinDataUrl
  })
}

export function resolveFace(username: string): Promise<string | null> {
  let cached = faceCache.get(username)
  if (!cached) {
    cached = invoke<SkinPreview>('preview_skin', {
      username,
      skinPath: null,
      capePath: null,
      slim: false,
    })
      .then((preview) => (preview.skin ? faceFromSkin(preview.skin) : null))
      .catch(() => null)
    faceCache.set(username, cached)
  }
  return cached
}
