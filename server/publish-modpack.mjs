import { readFile } from 'node:fs/promises'
import { basename } from 'node:path'

const DIRECTUS_URL = (process.env.DIRECTUS_URL ?? 'http://localhost:8055').replace(/\/$/, '')
const DIRECTUS_TOKEN = process.env.DIRECTUS_TOKEN ?? 'blockfield-dev-token'
const API_URL = (process.env.BLOCKFIELD_API_URL ?? 'http://localhost:3000/api/launcher/v1').replace(
  /\/$/,
  '',
)

const [zipPath, version, minecraftVersion = '1.20.1'] = process.argv.slice(2)

if (!zipPath || !version) {
  console.error('usage: pnpm cms:publish-modpack <zip-path> <version> [minecraft-version]')
  process.exit(1)
}

const file = await uploadFile(zipPath)
const release = await createRelease(file.id, version, minecraftVersion)
await reloadApi()

console.log(`Published modpack ${release.version} (${minecraftVersion}) with file ${file.id}.`)

async function uploadFile(path) {
  const data = await readFile(path)
  const form = new FormData()
  form.append('file', new Blob([data]), basename(path))

  const response = await fetch(`${DIRECTUS_URL}/files`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${DIRECTUS_TOKEN}` },
    body: form,
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`Directus file upload failed: ${JSON.stringify(body)}`)
  return body.data
}

async function createRelease(fileId, version, minecraftVersion) {
  const response = await fetch(`${DIRECTUS_URL}/items/modpack_releases`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${DIRECTUS_TOKEN}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      status: 'published',
      version,
      minecraft_version: minecraftVersion,
      modpack_zip: fileId,
    }),
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`Directus release create failed: ${JSON.stringify(body)}`)
  return body.data
}

async function reloadApi() {
  try {
    const response = await fetch(`${API_URL}/reload`, { method: 'POST' })
    if (!response.ok) console.warn(`API reload failed: ${response.status} ${await response.text()}`)
  } catch (error) {
    console.warn(`API reload skipped: ${error.message}`)
  }
}

async function readJson(response) {
  const text = await response.text()
  return text ? JSON.parse(text) : null
}
