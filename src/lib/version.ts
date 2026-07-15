import { useEffect, useState } from 'react'

export function useLauncherVersion() {
  const [version, setVersion] = useState('0.1.0')

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    import('@tauri-apps/api/app')
      .then(({ getVersion }) => getVersion())
      .then(setVersion)
      .catch(() => undefined)
  }, [])

  return version
}
