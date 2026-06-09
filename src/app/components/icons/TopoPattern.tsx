const RING_COUNT = 18;
const RING_BASE = 60;
const RING_STEP = 28;
const LINE_COUNT = 12;
const LINE_OFFSET = 80;
const LINE_STEP = 110;

export function TopoPattern() {
  return (
    <svg
      aria-hidden
      className="absolute inset-0 w-full h-full pointer-events-none opacity-[0.07]"
      preserveAspectRatio="none"
      viewBox="0 0 1280 760"
    >
      <defs>
        <pattern id="topo" x="0" y="0" width="1280" height="760" patternUnits="userSpaceOnUse">
          {Array.from({ length: RING_COUNT }, (_, i) => (
            <circle
              key={`r${i}`}
              cx="640"
              cy="380"
              r={RING_BASE + i * RING_STEP}
              fill="none"
              stroke="#F5A524"
              strokeWidth="0.5"
              strokeDasharray="2 3"
            />
          ))}
          {Array.from({ length: LINE_COUNT }, (_, i) => (
            <line
              key={`v${i}`}
              x1={LINE_OFFSET + i * LINE_STEP}
              y1="0"
              x2={LINE_OFFSET + i * LINE_STEP}
              y2="760"
              stroke="#3A2C1D"
              strokeWidth="0.3"
            />
          ))}
        </pattern>
      </defs>
      <rect width="1280" height="760" fill="url(#topo)" />
    </svg>
  );
}
