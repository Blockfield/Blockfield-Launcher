import { readFile } from 'node:fs/promises'
import { basename } from 'node:path'

const CMS_URL = (process.env.CMS_URL ?? 'http://localhost:8055/api').replace(/\/$/, '')
const CMS_TOKEN = process.env.CMS_TOKEN
const RELOAD_TOKEN = process.env.RELOAD_TOKEN
const JAVA_VERSION = process.env.JAVA_VERSION
const JAVA_PLATFORM = process.env.JAVA_PLATFORM
const JAVA_URL = process.env.JAVA_URL
const JAVA_SHA256 = process.env.JAVA_SHA256
const JAVA_SIZE = Number(process.env.JAVA_SIZE)
const API_URL = (process.env.BLOCKFIELD_API_URL ?? 'http://localhost:3000/api/launcher/v1').replace(
  /\/$/,
  '',
)

const [zipPath, version, minecraftVersion = '1.20.1'] = process.argv.slice(2)

if (!zipPath || !version) {
  console.error('usage: pnpm cms:publish-modpack <zip-path> <version> [minecraft-version]')
  process.exit(1)
}
if (!CMS_TOKEN || !RELOAD_TOKEN) {
  throw new Error('CMS_TOKEN and RELOAD_TOKEN are required')
}
if (
  !JAVA_VERSION ||
  !JAVA_PLATFORM ||
  !JAVA_URL ||
  !JAVA_SHA256 ||
  !Number.isSafeInteger(JAVA_SIZE)
) {
  throw new Error('JAVA_VERSION, JAVA_PLATFORM, JAVA_URL, JAVA_SHA256, and JAVA_SIZE are required')
}

const file = await uploadFile(zipPath)
const release = await createRelease(file.id, version, minecraftVersion)
await reloadApi(release.id)
await activateRelease(release.id)

console.log(`Published modpack ${release.version} (${minecraftVersion}) with file ${file.id}.`)

async function uploadFile(path) {
  const data = await readFile(path)
  const form = new FormData()
  form.append('file', new Blob([data]), basename(path))

  const response = await cmsFetch('/files', {
    method: 'POST',
    body: form,
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`CMS file upload failed: ${JSON.stringify(body)}`)
  return body.data
}

async function createRelease(fileId, version, minecraftVersion) {
  const response = await cmsFetch('/items/modpack_releases', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      version,
      minecraft_version: minecraftVersion,
      modpack_zip: fileId,
      java: {
        version: JAVA_VERSION,
        platform: JAVA_PLATFORM,
        url: JAVA_URL,
        sha256: JAVA_SHA256,
        size: JAVA_SIZE,
      },
      status: 'ready',
    }),
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`CMS release create failed: ${JSON.stringify(body)}`)
  return body.data
}

async function reloadApi(releaseId) {
  const response = await fetch(`${API_URL}/reload`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${RELOAD_TOKEN}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ releaseId }),
  })
  if (!response.ok)
    throw new Error(`API reload failed: ${response.status} ${await response.text()}`)
}

async function activateRelease(releaseId) {
  const response = await cmsFetch(`/items/modpack_releases/${releaseId}/activate`, {
    method: 'POST',
  })
  if (!response.ok)
    throw new Error(`CMS activation failed: ${response.status} ${await response.text()}`)
}

function cmsFetch(path, init) {
  return fetch(`${CMS_URL}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${CMS_TOKEN}`,
      ...init.headers,
    },
  })
}

async function readJson(response) {
  const text = await response.text()
  return text ? JSON.parse(text) : null
}
