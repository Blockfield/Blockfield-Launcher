export function ramLimitGb(maxRamMb = 32768): number {
  return Math.max(0, Math.min(32, Math.floor(maxRamMb / 1024)))
}

export function clampRamGb(value: number, max: number): number {
  return Math.min(max, Math.max(2, Number.isFinite(value) ? Math.round(value) : 2))
}
