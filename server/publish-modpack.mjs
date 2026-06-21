import { readFile } from 'node:fs/promises'
import { basename } from 'node:path'

const DIRECTUS_URL = (process.env.DIRECTUS_URL ?? 'http://localhost:8055').replace(/\/$/, '')
const DIRECTUS_TOKEN = process.env.DIRECTUS_TOKEN ?? 'blockfield-dev-token'
const DIRECTUS_ADMIN_EMAIL = process.env.DIRECTUS_ADMIN_EMAIL ?? 'admin@example.com'
const DIRECTUS_ADMIN_PASSWORD = process.env.DIRECTUS_ADMIN_PASSWORD ?? 'd1r3ctu5'
const API_URL = (process.env.BLOCKFIELD_API_URL ?? 'http://localhost:3000/api/launcher/v1').replace(
  /\/$/,
  '',
)
let directusToken = DIRECTUS_TOKEN
let triedAdminLogin = false

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

  const response = await directusFetch('/files', {
    method: 'POST',
    body: form,
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`Directus file upload failed: ${JSON.stringify(body)}`)
  return body.data
}

async function createRelease(fileId, version, minecraftVersion) {
  const response = await directusFetch('/items/modpack_releases', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
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

async function directusFetch(path, init) {
  let response = await fetch(`${DIRECTUS_URL}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${directusToken}`,
      ...init.headers,
    },
  })

  if ((response.status === 401 || response.status === 403) && !triedAdminLogin) {
    directusToken = await login()
    triedAdminLogin = true
    response = await fetch(`${DIRECTUS_URL}${path}`, {
      ...init,
      headers: {
        Authorization: `Bearer ${directusToken}`,
        ...init.headers,
      },
    })
  }

  return response
}

async function login() {
  const response = await fetch(`${DIRECTUS_URL}/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      email: DIRECTUS_ADMIN_EMAIL,
      password: DIRECTUS_ADMIN_PASSWORD,
    }),
  })
  const body = await readJson(response)
  if (!response.ok) throw new Error(`Directus login failed: ${JSON.stringify(body)}`)

  const token = body.data?.access_token
  if (!token) throw new Error('Directus login did not return an access token')
  return token
}

async function readJson(response) {
  const text = await response.text()
  return text ? JSON.parse(text) : null
}
