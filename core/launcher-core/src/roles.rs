//! Workshop role gating, ported from `src/lib/workshop.ts:1-7`
//! (used by `src/app/App.tsx:103` to gate the Workshop profile tab).

/// Authoritative map-edit site roles permitted to access the Workshop profile.
pub const WORKSHOP_ROLES: [&str; 4] = ["builder", "moderator", "admin", "owner"];

/// `hasWorkshopAccess`: only a loaded canonical role passes; `None` covers
/// TS `null`/`undefined`, and `Some("")` is falsy there too.
pub fn has_workshop_access(role: Option<&str>) -> bool {
    role.is_some_and(|role| WORKSHOP_ROLES.contains(&role))
}
