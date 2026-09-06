import type { ReactNode } from 'react'

export function SectionHeader({ label, code }: { label: string; code: string }) {
  return (
    <div className="flex min-w-0 items-center gap-3">
      <span className="size-1.5 bg-[#F5A524]" />
      <span className="text-[10px] tracking-[0.18em] text-[#F3E7D0]">{label}</span>
      <span className="h-px flex-1 bg-[#18130D]" />
      <span className="text-[9px] tracking-[0.28em] text-[#5E5040]">{code}</span>
    </div>
  )
}

export function StatusDot({
  color = '#82D66B',
  pulse = false,
}: {
  color?: string
  pulse?: boolean
}) {
  return (
    <span
      className={`inline-block size-1.5 shrink-0 rounded-full align-middle ${pulse ? 'animate-pulse' : ''}`}
      style={{ background: color, boxShadow: `0 0 6px ${color}b3` }}
    />
  )
}

export function OperationBar({
  label,
  status,
  statusColor = '#82D66B',
  align = 'left',
}: {
  label: string
  status?: ReactNode
  statusColor?: string
  align?: 'left' | 'right'
}) {
  return (
    <div className="flex items-center gap-3 mb-3">
      {align === 'left' ? (
        <>
          <span className="shrink-0 text-[10px] tracking-[0.28em] text-[#8E7A5E]">{label}</span>
          <span className="h-px min-w-4 flex-1 bg-[#18130D]" />
          {status != null && (
            <span
              className="inline-flex min-w-0 shrink-0 items-center gap-2 whitespace-nowrap text-[10px] leading-none tracking-[0.24em]"
              style={{ color: statusColor }}
            >
              <StatusDot color={statusColor} pulse />
              {status}
            </span>
          )}
        </>
      ) : (
        <>
          <span className="shrink-0 text-[10px] tracking-[0.28em] text-[#8E7A5E]">{label}</span>
          <span className="h-px min-w-4 flex-1 bg-[#18130D]" />
          <span className="shrink-0 text-[10px] tracking-[0.28em] text-[#8E7A5E]">
            <span className="break-words">{status}</span>
          </span>
        </>
      )}
    </div>
  )
}

export function GlowPanel({
  children,
  glow = true,
  className = '',
}: {
  children: ReactNode
  glow?: boolean
  className?: string
}) {
  return (
    <div className="relative mt-4">
      <div
        className={`absolute -inset-px pointer-events-none border ${
          glow ? 'border-[#F5A524]/20' : 'border-[#2A2116]/60'
        }`}
      />
      <div
        className={`relative border border-[#2A2116] bg-gradient-to-br from-[#11100D] via-[#11100D] to-[#0B0906] ${className}`}
      >
        {children}
      </div>
    </div>
  )
}
