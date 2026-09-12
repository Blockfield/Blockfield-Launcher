import { useState } from 'react'
import defaultHead from '../../assets/default-player-head.svg'

export function PlayerHead({ username }: { username: string }) {
  const [failedUsername, setFailedUsername] = useState<string | null>(null)
  const canLoad = /^[A-Za-z0-9_]{3,16}$/.test(username) && failedUsername !== username

  return (
    <div className="relative size-6 shrink-0">
      <img
        src={defaultHead}
        alt=""
        className="absolute inset-0 size-6 [image-rendering:pixelated]"
      />
      {canLoad && (
        <img
          key={username}
          src={`https://mc-heads.net/avatar/${encodeURIComponent(username)}/24`}
          alt=""
          className="absolute inset-0 size-6 [image-rendering:pixelated]"
          onError={() => setFailedUsername(username)}
        />
      )}
    </div>
  )
}
