import { describe, expect, it } from 'vitest'
import { clampRamGb, ramLimitGb } from './ram'

describe('RAM limits', () => {
  it('keeps whole-GB choices below the reported Windows limit', () => {
    const limit = ramLimitGb(12221)
    expect(limit).toBe(11)
    expect(clampRamGb(12, limit) * 1024).toBeLessThanOrEqual(12221)
  })

  it('handles exact limits, UI cap and unavailable memory', () => {
    expect(ramLimitGb(12288)).toBe(12)
    expect(ramLimitGb(65536)).toBe(32)
    expect(ramLimitGb()).toBe(32)
    expect(ramLimitGb(2047)).toBe(1)
  })

  it('bounds manual input and saved values', () => {
    expect(clampRamGb(24, 11)).toBe(11)
    expect(clampRamGb(0, 11)).toBe(2)
    expect(clampRamGb(Number.NaN, 11)).toBe(2)
    expect(clampRamGb(4, 2)).toBe(2)
  })
})
