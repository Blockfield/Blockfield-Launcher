import { readFile } from 'node:fs/promises'

const packageVersion = JSON.parse(
  await readFile(new URL('../package.json', import.meta.url)),
).version
const tauriVersion = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url)),
).version
const cargo = await readFile(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8')
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]

if (!packageVersion || packageVersion !== tauriVersion || packageVersion !== cargoVersion) {
  throw new Error(
    `Launcher version mismatch: package=${packageVersion}, tauri=${tauriVersion}, cargo=${cargoVersion}`,
  )
}

console.log(`Launcher version ${packageVersion} is synchronized.`)
