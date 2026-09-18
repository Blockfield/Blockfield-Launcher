import { useEffect, useState, useSyncExternalStore } from 'react'
import defaultHead from '../../assets/default-player-head.svg'
import { getFaceVersion, resolveFace, subscribeFaces } from '../../lib/player-face'

const isTauri = () => '__TAURI_INTERNALS__' in window

export function PlayerHead({ username }: { username: string }) {
  const [resolved, setResolved] = useState<{ username: string; face: string | null } | null>(null)
  const face = resolved?.username === username ? resolved.face : null
  const version = useSyncExternalStore(subscribeFaces, getFaceVersion, getFaceVersion)

  useEffect(() => {
    if (!isTauri() || !/^[A-Za-z0-9_]{3,16}$/.test(username)) return
    let active = true
    void resolveFace(username).then((url) => active && setResolved({ username, face: url }))
    return () => {
      active = false
    }
  }, [username, version])

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
