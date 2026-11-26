import { readFile } from 'node:fs/promises'

const packageVersion = JSON.parse(
  await readFile(new URL('../package.json', import.meta.url)),
).version
const tauriVersion = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url)),
).version
const cargo = await readFile(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8')
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]

let nativeVersion = packageVersion
try {
  const native = await readFile(new URL('../native/app/Cargo.toml', import.meta.url), 'utf8')
  nativeVersion = native.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
} catch {
  // The native shell predates some tags: only check it when present.
}

if (
  !packageVersion ||
  packageVersion !== tauriVersion ||
  packageVersion !== cargoVersion ||
  packageVersion !== nativeVersion
) {
  throw new Error(
    `Launcher version mismatch: package=${packageVersion}, tauri=${tauriVersion}, cargo=${cargoVersion}, native=${nativeVersion}`,
  )
}

console.log(`Launcher version ${packageVersion} is synchronized.`)
