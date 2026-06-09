export function GridBackdrop({ intensity = 1 }: { intensity?: number }) {
  const op = 0.06 * intensity;
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
        backgroundSize: "48px 48px, 48px 48px, 100% 100%, 100% 100%",
      }}
    />
  );
}

export function TopoBackdrop() {
  return (
    <svg
      aria-hidden
      className="absolute inset-0 w-full h-full pointer-events-none opacity-[0.07]"
      preserveAspectRatio="none"
      viewBox="0 0 1280 760"
    >
      <defs>
        <pattern id="topo" x="0" y="0" width="1280" height="760" patternUnits="userSpaceOnUse">
          {Array.from({ length: 18 }).map((_, i) => {
            const r = 60 + i * 28;
            return (
              <circle
                key={i}
                cx="640"
                cy="380"
                r={r}
                fill="none"
                stroke="#F5A524"
                strokeWidth="0.5"
                strokeDasharray="2 3"
              />
            );
          })}
          {Array.from({ length: 12 }).map((_, i) => {
            const x = 80 + i * 110;
            return (
              <line
                key={`v${i}`}
                x1={x}
                y1="0"
                x2={x}
                y2="760"
                stroke="#3A2C1D"
                strokeWidth="0.3"
              />
            );
          })}
        </pattern>
      </defs>
      <rect width="1280" height="760" fill="url(#topo)" />
    </svg>
  );
}

export function CornerTicks({ className = "" }: { className?: string }) {
  return (
    <div className={`pointer-events-none absolute inset-0 ${className}`}>
      <Tick className="top-0 left-0" />
      <Tick className="top-0 right-0 rotate-90" />
      <Tick className="bottom-0 left-0 -rotate-90" />
      <Tick className="bottom-0 right-0 rotate-180" />
    </div>
  );
}

function Tick({ className = "" }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      className={`absolute ${className}`}
      fill="none"
    >
      <path d="M0 0H6" stroke="#F5A524" strokeWidth="1" />
      <path d="M0 0V6" stroke="#F5A524" strokeWidth="1" />
    </svg>
  );
}
