/** Mirrors the backend rule in src-tauri/src/commands.rs::valid_username. */
const USERNAME_RE = /^[A-Za-z0-9_]{3,16}$/

export function isValidUsername(name: string): boolean {
  return USERNAME_RE.test(name.trim())
}

export const USERNAME_HINT = 'Ник в Minecraft (3-16 букв, цифр или _).'
