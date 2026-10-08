import { describe, expect, it } from 'vitest'
import {
  DEFAULT_WORKSHOP_SERVER,
  hasWorkshopAccess,
  WORKSHOP_ROLES,
} from './workshop'

describe('hasWorkshopAccess', () => {
  it('grants access to canonical site roles with map-edit permissions', () => {
    for (const role of WORKSHOP_ROLES) {
      expect(hasWorkshopAccess(role)).toBe(true)
    }
    expect(hasWorkshopAccess('builder')).toBe(true)
    expect(hasWorkshopAccess('moderator')).toBe(true)
    expect(hasWorkshopAccess('admin')).toBe(true)
    expect(hasWorkshopAccess('owner')).toBe(true)
  })

  it('denies access to unloaded, missing, ordinary, or unknown roles', () => {
    expect(hasWorkshopAccess(undefined)).toBe(false)
    expect(hasWorkshopAccess(null)).toBe(false)
    expect(hasWorkshopAccess('')).toBe(false)
    expect(hasWorkshopAccess('player')).toBe(false)
    expect(hasWorkshopAccess('игрок')).toBe(false)
    expect(hasWorkshopAccess('user')).toBe(false)
    expect(hasWorkshopAccess('vip')).toBe(false)
    expect(hasWorkshopAccess('unknown')).toBe(false)
  })

  it('provides the reliable default first-party workshop server address', () => {
    expect(DEFAULT_WORKSHOP_SERVER).toBe('workshop.blockfield.pro:25565')
  })
})
