const DIRECTUS_URL = (process.env.DIRECTUS_URL ?? 'http://localhost:8055').replace(/\/$/, '')
const DIRECTUS_TOKEN = process.env.DIRECTUS_TOKEN ?? 'blockfield-dev-token'
const DIRECTUS_ADMIN_EMAIL = process.env.DIRECTUS_ADMIN_EMAIL ?? 'admin@localhost'
const DIRECTUS_ADMIN_PASSWORD = process.env.DIRECTUS_ADMIN_PASSWORD ?? 'd1r3ctu5'
let directusToken = DIRECTUS_TOKEN
let triedAdminLogin = false

function env(key, fallback) {
  const val = process.env[key]
  return val != null && val !== '' ? val : fallback
}

const statusChoices = [
  { text: 'Draft', value: 'draft' },
  { text: 'Published', value: 'published' },
]

const fields = {
  id: {
    field: 'id',
    type: 'integer',
    meta: { interface: 'input', hidden: true, readonly: true },
    schema: { is_primary_key: true, has_auto_increment: true },
  },
  status: {
    field: 'status',
    type: 'string',
    meta: { interface: 'select-dropdown', options: { choices: statusChoices }, width: 'half' },
    schema: { default_value: 'draft' },
  },
}

const collections = [
  {
    name: 'modpack_releases',
    icon: 'deployed_code',
    note: 'Published Minecraft builds for the launcher.',
    fields: [
      fields.id,
      fields.status,
      textField('version', 'Version', true),
      textField('minecraft_version', 'Minecraft version', true),
      fileField('modpack_zip', 'Modpack ZIP uploaded to Directus'),
      textField('zip_url', 'External ZIP URL fallback'),
      jsonField('prune', 'Paths to remove before installing this release'),
      jsonField('java', 'Java runtime object'),
      jsonField('forge', 'Forge installer object'),
    ],
    fileRelations: ['modpack_zip'],
  },
  {
    name: 'launcher_updates',
    icon: 'system_update',
    note: 'Tauri launcher updater feed.',
    fields: [
      fields.id,
      fields.status,
      textField('version', 'Launcher version', true),
      textField('pub_date', 'Publication date'),
      textField('windows_url', 'Windows installer URL'),
      textField('windows_signature', 'Windows updater signature'),
      textArea('notes', 'Release notes'),
      jsonField('platforms', 'Raw Tauri updater platforms object'),
    ],
  },
  {
    name: 'launcher_content',
    icon: 'dashboard_customize',
    note: 'Visible launcher copy and dashboard content.',
    fields: [
      fields.id,
      fields.status,
      textField('brand', 'Brand'),
      textField('brand_subtitle', 'Brand subtitle'),
      textField('chrome_title', 'Window title'),
      textField('operation_name', 'Operation name'),
      textField('season', 'Season label'),
      textArea('description', 'Main screen description'),
      textField('server_name', 'Server name'),
      textField('server_ip', 'Server IP'),
      textField('server_region', 'Login region label'),
      textField('operators', 'Online operators display'),
      textField('ping', 'Ping display'),
      textField('region', 'Region display'),
      textField('launcher_version', 'Displayed launcher version'),
      textField('coordinates', 'Login coordinates'),
      textField('copyright', 'Copyright footer'),
      textField('login_sector', 'Login sector text'),
      textField('login_slogan', 'Login slogan'),
      textField('operator_handle', 'Operator handle'),
      textField('operator_initials', 'Operator initials'),
      textField('operator_rank', 'Operator rank'),
      textField('support_label', 'Support link label'),
      textField('network_status', 'Network status label'),
      textArea('update_description', 'Update screen description'),
      textField('settings_preferences', 'Settings screen preference label'),
      jsonField('translations', 'Optional UI text overrides by language and i18n key'),
      jsonField('features', 'Main screen feature cards'),
      jsonField('feed', 'Main screen news feed'),
    ],
  },
]

async function main() {
  await waitForDirectus()
  await ensureStaticToken()

  for (const collection of collections) {
    await ensureCollection(collection)
    for (const field of collection.fields) {
      await ensureField(collection.name, field)
    }
    for (const field of collection.fileRelations ?? []) {
      await ensureFileRelation(collection.name, field)
    }
  }

  await seedContent()
  await seedLauncherUpdate()
  console.log('Directus CMS model is ready.')
}

async function ensureStaticToken() {
  await directus('/users/me')

  if (directusToken === DIRECTUS_TOKEN) return

  await directus('/users/me', {
    method: 'PATCH',
    body: JSON.stringify({ token: DIRECTUS_TOKEN }),
  })
  console.log('admin static token configured')
}

function textField(field, note, required = false) {
  return {
    field,
    type: 'string',
    meta: { interface: 'input', note, required, width: 'half' },
  }
}

function textArea(field, note) {
  return {
    field,
    type: 'text',
    meta: { interface: 'input-multiline', note, width: 'full' },
  }
}

function jsonField(field, note) {
  return {
    field,
    type: 'json',
    meta: { interface: 'input-code', note, width: 'full' },
  }
}

function fileField(field, note) {
  return {
    field,
    type: 'uuid',
    meta: { interface: 'file', note, width: 'half' },
  }
}

async function waitForDirectus() {
  for (let i = 0; i < 30; i += 1) {
    try {
      await directus('/collections')
      return
    } catch (error) {
      if (i === 29) throw error
      await new Promise((resolve) => setTimeout(resolve, 1000))
    }
  }
}

async function ensureCollection({ name, icon, note }) {
  if (await exists(`/collections/${name}`)) {
    console.log(`collection exists: ${name}`)
    return
  }

  await directus('/collections', {
    method: 'POST',
    body: JSON.stringify({
      collection: name,
      meta: { collection: name, icon, note },
      schema: { name },
    }),
  })
  console.log(`collection created: ${name}`)
}

async function ensureField(collection, field) {
  if (await exists(`/fields/${collection}/${field.field}`)) return

  await directus(`/fields/${collection}`, {
    method: 'POST',
    body: JSON.stringify(field),
  })
  console.log(`field created: ${collection}.${field.field}`)
}

async function ensureFileRelation(collection, field) {
  const relations = await directus('/relations?limit=-1')
  if (
    relations.data?.some(
      (relation) =>
        (relation.collection ?? relation.collection_many ?? relation.many_collection) ===
          collection &&
        (relation.field ?? relation.field_many ?? relation.many_field) === field &&
        (relation.related_collection ?? relation.collection_one ?? relation.one_collection) ===
          'directus_files',
    )
  ) {
    return
  }

  try {
    await directus('/relations', {
      method: 'POST',
      body: JSON.stringify({
        collection,
        field,
        related_collection: 'directus_files',
        meta: {
          many_collection: collection,
          many_field: field,
          one_collection: 'directus_files',
        },
      }),
    })
    console.log(`relation created: ${collection}.${field} -> directus_files`)
  } catch (error) {
    console.warn(`file relation skipped for ${collection}.${field}: ${error.message}`)
  }
}

async function seedContent() {
  if (await hasItems('launcher_content')) return

  await directus('/items/launcher_content', {
    method: 'POST',
    body: JSON.stringify({
      status: 'published',
      brand: env('SEED_BRAND', 'BLOCKFIELD'),
      brand_subtitle: env('SEED_BRAND_SUBTITLE', 'TACTICAL OPS'),
      chrome_title: env('SEED_CHROME_TITLE', 'BLOCKFIELD LAUNCHER'),
      operation_name: env('SEED_OPERATION_NAME', 'IRON FRONT'),
      season: env('SEED_SEASON', '/ SEASON 01'),
      description: env('SEED_DESCRIPTION',
        'Large-scale tactical PvP across contested terrain. Capture points, coordinate with your squad, and deploy armored vehicles.'),
      server_name: env('SEED_SERVER_NAME', 'BLOCKFIELD - PRIMARY'),
      server_ip: env('SEED_SERVER_IP', 'play.blockfield.gg:25565'),
      server_region: env('SEED_SERVER_REGION', 'EU-WEST - 28ms'),
      operators: env('SEED_OPERATORS', '142'),
      ping: env('SEED_PING', '28'),
      region: env('SEED_REGION', 'EU-W'),
      launcher_version: env('SEED_LAUNCHER_VERSION', '0.4.2'),
      coordinates: env('SEED_COORDINATES', 'LAT 47.3829 / LON 19.0402'),
      copyright: env('SEED_COPYRIGHT', '2026 BLOCKFIELD COMMAND'),
      login_sector: env('SEED_LOGIN_SECTOR', 'SECTOR 07 - NORTH RIDGE'),
      login_slogan: env('SEED_LOGIN_SLOGAN', 'DEPLOY. CAPTURE. DOMINATE.'),
      operator_handle: env('SEED_OPERATOR_HANDLE', 'KILO_7'),
      operator_initials: env('SEED_OPERATOR_INITIALS', 'K7'),
      operator_rank: env('SEED_OPERATOR_RANK', 'RANK - SERGEANT'),
      support_label: env('SEED_SUPPORT_LABEL', 'SUPPORT'),
      network_status: env('SEED_NETWORK_STATUS', 'NETWORK NOMINAL'),
      update_description: env('SEED_UPDATE_DESCRIPTION',
        'Synchronizing modpack assets with the primary deployment server. Do not close the launcher until the operation completes.'),
      settings_preferences: env('SEED_SETTINGS_PREFERENCES', '/ LAUNCHER PREFERENCES'),
      translations: {
        en: {},
        ru: {},
        uk: {},
      },
      features: [
        {
          icon: 'flag',
          title: 'CAPTURE POINTS',
          desc: 'Dynamic objective control across multiple sectors.',
        },
        {
          icon: 'swords',
          title: '6 CLASSES',
          desc: 'Assault, Recon, Engineer, Medic, Support, Pilot.',
        },
        {
          icon: 'truck',
          title: 'ARMORED VEHICLES',
          desc: 'Tanks, APCs, light recon and air transport.',
        },
        {
          icon: 'crosshair',
          title: 'TACTICAL BATTLES',
          desc: 'Squad-based 64v64 persistent warfare.',
        },
      ],
      feed: [
        {
          tag: 'PATCH',
          tone: 'amber',
          date: '06.07',
          title: '0.1.43 - Vehicle Balance',
          body: 'New modpack release is available.',
        },
      ],
    }),
  })
  console.log('seeded launcher_content')
}

async function seedLauncherUpdate() {
  if (await hasItems('launcher_updates')) return

  await directus('/items/launcher_updates', {
    method: 'POST',
    body: JSON.stringify({
      status: 'published',
      version: env('SEED_UPDATE_VERSION', '0.1.0'),
      notes: env('SEED_UPDATE_NOTES', 'No launcher update available.'),
      pub_date: env('SEED_UPDATE_PUB_DATE', '2026-06-18T00:00:00Z'),
      windows_url: env('SEED_UPDATE_WINDOWS_URL', 'https://play.blockfield.gg/downloads/blockfield-launcher_0.1.0_x64-setup.exe'),
      windows_signature: env('SEED_UPDATE_WINDOWS_SIGNATURE', ''),
    }),
  })
  console.log('seeded launcher_updates')
}

async function hasItems(collection) {
  const response = await directus(`/items/${collection}?limit=1&fields=id`)
  return response.data?.length > 0
}

async function exists(path) {
  try {
    await directus(path)
    return true
  } catch (error) {
    if (error.status === 404 || error.status === 403) return false
    throw error
  }
}

async function directus(path, init = {}) {
  let response = await directusFetch(path, init)

  if ((response.status === 401 || response.status === 403) && !triedAdminLogin) {
    directusToken = await login()
    triedAdminLogin = true
    response = await directusFetch(path, init)
  }

  if (!response.ok) {
    const error = new Error(await response.text())
    error.status = response.status
    throw error
  }

  if (response.status === 204) return null
  return response.json()
}

async function directusFetch(path, init = {}) {
  return fetch(`${DIRECTUS_URL}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${directusToken}`,
      'Content-Type': 'application/json',
      ...init.headers,
    },
  })
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

  if (!response.ok) {
    const error = new Error(await response.text())
    error.status = response.status
    throw error
  }

  const body = await response.json()
  const token = body.data?.access_token
  if (!token) throw new Error('Directus login did not return an access token')
  return token
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
