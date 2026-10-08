export const WORKSHOP_ROLES = ['builder', 'moderator', 'admin', 'owner'] as const
export type WorkshopRole = (typeof WORKSHOP_ROLES)[number]

export const DEFAULT_WORKSHOP_SERVER = 'workshop.blockfield.pro:25565'

/**
 * Checks whether an authoritative account role has access to the Workshop profile.
 * Canonical site roles builder, moderator, admin, and owner all have map-edit permissions.
 * Ordinary players, users without roles, and unloaded profiles have no map-edit access.
 */
export function hasWorkshopAccess(role: string | null | undefined): boolean {
  return Boolean(role && (WORKSHOP_ROLES as readonly string[]).includes(role))
}
