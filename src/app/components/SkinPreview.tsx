import { useEffect, useRef } from 'react'
import { SkinViewer, WalkingAnimation } from 'skinview3d'
import type { SkinPreview as SkinPreviewData } from '../../lib/api'

const WIDTH = 220
const HEIGHT = 300

export function SkinPreview({
  data,
  caption,
  fallback,
}: {
  data: SkinPreviewData | null
  caption: string
  fallback: string
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const viewerRef = useRef<SkinViewer | null>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    let viewer: SkinViewer
    try {
      viewer = new SkinViewer({ canvas, width: WIDTH, height: HEIGHT, zoom: 0.85 })
    } catch (e) {
      // No WebGL (software WebKitGTK, broken driver): the caption alone stays visible.
      console.error(e)
      return
    }
    viewer.animation = new WalkingAnimation()
    viewer.animation.speed = 0.6
    viewer.autoRotate = true
    viewer.autoRotateSpeed = 0.6
    viewer.controls.enableZoom = false
    viewerRef.current = viewer
    return () => {
      viewer.dispose()
      viewerRef.current = null
    }
  }, [])

  useEffect(() => {
    const viewer = viewerRef.current
    if (!viewer) return
    if (data?.skin) {
      viewer.loadSkin(data.skin, { model: data.slim ? 'slim' : 'default' }).catch(console.error)
    } else {
      viewer.resetSkin()
    }
    if (data?.cape) {
      viewer.loadCape(data.cape).catch(console.error)
    } else {
      viewer.resetCape()
    }
  }, [data])

  return (
    <div className="flex flex-col items-center gap-2 border border-[#2A2116] bg-[#0B0906] p-2">
      <canvas
        ref={canvasRef}
        width={WIDTH}
        height={HEIGHT}
        aria-label={caption}
        className="cursor-grab active:cursor-grabbing"
      />
      <span className="text-[10px] tracking-[0.18em] text-[#8E7A5E]">
        {data?.skin ? caption : fallback}
      </span>
    </div>
  )
}
