import { describe, expect, it } from 'vitest'
import { isValidUsername } from './username'

describe('isValidUsername', () => {
  it('accepts 3-16 alphanumeric/underscore names', () => {
    expect(isValidUsername('Steve')).toBe(true)
    expect(isValidUsername('a_1')).toBe(true)
    expect(isValidUsername('a'.repeat(16))).toBe(true)
  })

  it('rejects names outside the backend rule', () => {
    expect(isValidUsername('')).toBe(false)
    expect(isValidUsername('ab')).toBe(false)
    expect(isValidUsername('a'.repeat(17))).toBe(false)
    expect(isValidUsername('bad name')).toBe(false)
    expect(isValidUsername('bad-name')).toBe(false)
  })

  it('trims surrounding whitespace before validating', () => {
    expect(isValidUsername('  Steve  ')).toBe(true)
  })
})
