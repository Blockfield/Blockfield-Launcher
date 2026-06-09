import { Minus, Square, X } from 'lucide-react'
import { useState, useEffect, type ReactNode } from 'react'
import type { Window as TauriWindow } from '@tauri-apps/api/window'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export function WindowChrome({ children }: { children: ReactNode }) {
  const [appWindow, setAppWindow] = useState<TauriWindow | null>(null)

  useEffect(() => {
    if (isTauri) {
      import('@tauri-apps/api/window').then((mod) => {
        setAppWindow(mod.getCurrentWindow())
      })
    }
  }, [])

  const handleMinimize = async () => {
    if (appWindow) await appWindow.minimize()
  }

  const handleMaximize = async () => {
    if (appWindow) await appWindow.toggleMaximize()
  }

  const handleClose = async () => {
    if (appWindow) await appWindow.close()
  }

  if (isTauri) {
    return (
      <div
        className="relative w-screen h-screen overflow-hidden rounded-md border border-[#2A2116] bg-[#070604] text-neutral-200 flex flex-col"
        style={{
          boxShadow: '0 30px 80px -20px rgba(0,0,0,0.82), 0 0 0 1px rgba(255,255,255,0.02) inset',
        }}
      >
        <div
          data-tauri-drag-region
          className="h-9 flex items-center justify-between border-b border-[#18130D] bg-[#0B0906] px-3 select-none shrink-0"
        >
          <div
            data-tauri-drag-region
            className="flex items-center gap-2 text-[10px] tracking-[0.3em] text-[#8E7A5E]"
          >
            <span className="size-1.5 rounded-full bg-[#F5A524]" />
            BLOCKFIELD LAUNCHER
          </div>
          <div className="flex items-center gap-1 text-[#8E7A5E] z-50">
            <button
              onClick={handleMinimize}
              className="h-6 w-8 grid place-items-center hover:bg-[#18130D] rounded-sm transition-colors cursor-pointer"
            >
              <Minus size={12} />
            </button>
            <button
              onClick={handleMaximize}
              className="h-6 w-8 grid place-items-center hover:bg-[#18130D] rounded-sm transition-colors cursor-pointer"
            >
              <Square size={10} />
            </button>
            <button
              onClick={handleClose}
              className="h-6 w-8 grid place-items-center hover:bg-[#5A1714] hover:text-white rounded-sm transition-colors cursor-pointer"
            >
              <X size={12} />
            </button>
          </div>
        </div>
        <div className="relative flex-1 min-h-0 overflow-hidden">{children}</div>
      </div>
    )
  }

  return (
    <div className="size-full min-w-[1280px] min-h-[720px] flex items-center justify-center bg-black/60 p-6">
      <div
        className="relative w-full h-full min-w-[1232px] min-h-[752px] overflow-hidden rounded-md border border-[#2A2116] bg-[#070604] text-neutral-200"
        style={{
          boxShadow: '0 30px 80px -20px rgba(0,0,0,0.82), 0 0 0 1px rgba(255,255,255,0.02) inset',
        }}
      >
        <div className="h-9 flex items-center justify-between border-b border-[#18130D] bg-[#0B0906] px-3 select-none">
          <div className="flex items-center gap-2 text-[10px] tracking-[0.3em] text-[#8E7A5E]">
            <span className="size-1.5 rounded-full bg-[#F5A524]" />
            BLOCKFIELD LAUNCHER
          </div>
          <div className="flex items-center gap-1 text-[#8E7A5E]">
            <button className="h-6 w-8 grid place-items-center hover:bg-[#18130D] rounded-sm transition-colors">
              <Minus size={12} />
            </button>
            <button className="h-6 w-8 grid place-items-center hover:bg-[#18130D] rounded-sm transition-colors">
              <Square size={10} />
            </button>
            <button className="h-6 w-8 grid place-items-center hover:bg-[#5A1714] hover:text-white rounded-sm transition-colors">
              <X size={12} />
            </button>
          </div>
        </div>
        <div className="relative h-[calc(100%-2.25rem)]">{children}</div>
      </div>
    </div>
  )
}
