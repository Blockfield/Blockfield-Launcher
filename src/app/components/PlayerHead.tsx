import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import defaultHead from '../../assets/default-player-head.svg'
import type { SkinPreview } from '../../lib/api'

const isTauri = () => '__TAURI_INTERNALS__' in window

// mc-heads.net only knows Mojang accounts, so Drasl-only players with a custom skin never showed
// it here; resolve through the same Drasl/Mojang chain as the skin preview and crop the face.
const faceCache = new Map<string, Promise<string | null>>()

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

function resolveFace(username: string): Promise<string | null> {
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

export function PlayerHead({ username }: { username: string }) {
  const [resolved, setResolved] = useState<{ username: string; face: string | null } | null>(null)
  const face = resolved?.username === username ? resolved.face : null

  useEffect(() => {
    if (!isTauri() || !/^[A-Za-z0-9_]{3,16}$/.test(username)) return
    let active = true
    void resolveFace(username).then((url) => active && setResolved({ username, face: url }))
    return () => {
      active = false
    }
  }, [username])

  return (
    <div className="relative size-6 shrink-0">
      <img
        src={defaultHead}
        alt=""
        className="absolute inset-0 size-6 [image-rendering:pixelated]"
      />
      {face && (
        <img
          key={username}
          src={face}
          alt=""
          className="absolute inset-0 size-6 [image-rendering:pixelated]"
        />
      )}
    </div>
  )
}
