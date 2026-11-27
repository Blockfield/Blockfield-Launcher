import { Tick } from './icons'

export { TopoPattern as TopoBackdrop } from './icons'

export function GridBackdrop({ intensity = 1 }: { intensity?: number }) {
  const op = 0.06 * intensity
  return (
    <div
      aria-hidden
      className="absolute inset-0 pointer-events-none"
      style={{
        backgroundImage: `
          linear-gradient(rgba(245,165,36,${op}) 1px, transparent 1px),
          linear-gradient(90deg, rgba(245,165,36,${op}) 1px, transparent 1px),
          radial-gradient(circle at 20% 20%, rgba(245,165,36,0.05), transparent 50%),
          radial-gradient(circle at 80% 80%, rgba(130,214,107,0.04), transparent 55%)
        `,
        backgroundSize: '48px 48px, 48px 48px, 100% 100%, 100% 100%',
      }}
    />
  )
}

export function CornerTicks({ className = '' }: { className?: string }) {
  return (
    <div className={`pointer-events-none absolute inset-0 ${className}`}>
      <Tick className="top-0 left-0" />
      <Tick className="top-0 right-0 rotate-90" />
      <Tick className="bottom-0 left-0 -rotate-90" />
      <Tick className="bottom-0 right-0 rotate-180" />
    </div>
  )
}
