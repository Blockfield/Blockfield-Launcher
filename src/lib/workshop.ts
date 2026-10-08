export const WORKSHOP_ROLES = ['builder', 'moderator', 'admin', 'owner'] as const
export type WorkshopRole = (typeof WORKSHOP_ROLES)[number]

/** Authoritative map-edit site roles permitted to access the Workshop profile. */
export function hasWorkshopAccess(role: string | null | undefined): boolean {
  return Boolean(role && (WORKSHOP_ROLES as readonly string[]).includes(role))
}
