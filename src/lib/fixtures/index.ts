import { invoke } from '@tauri-apps/api/core'
import { CAPTURE_STATES, FIXED_TIME, fixtureResponse, type CaptureState } from './data'
import skin from './skin.png?inline'
import cape from './cape.png?inline'
import './capture.css'

interface Selection {
  state: CaptureState
  lang: string
  scale: number
  motion: boolean
}
interface Internals {
  invoke: typeof invoke
  runCallback: (id: number, event: unknown) => void
}
const internals = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__
const nativeInvoke = internals.invoke.bind(internals)
const events = new Map<number, string>()
const errors: string[] = []
let selection: Selection
let nativeConfig: Record<string, unknown>

function emit(event: string, payload: unknown) {
  for (const [handler, name] of events) {
    if (name === event) internals.runCallback(handler, { event, id: handler, payload })
  }
}

export async function install() {
  nativeConfig = await nativeInvoke<Record<string, unknown>>('load_settings')
  const params = new URLSearchParams(location.search)
  selection = params.has('state')
    ? {
        state: params.get('state') as CaptureState,
        lang: params.get('lang') ?? 'ru',
        scale: Number(params.get('scale') ?? 1),
        motion: params.get('motion') === '1',
      }
    : (JSON.parse(
        String(nativeConfig.lang).startsWith('capture-selection:')
          ? String(nativeConfig.lang).slice('capture-selection:'.length)
          : String(nativeConfig.username),
      ) as Selection)
  if (!CAPTURE_STATES.includes(selection.state)) throw new Error('Unknown capture state')
  await nativeInvoke('plugin:webview|set_webview_zoom', { label: 'main', value: selection.scale })
  if (!selection.motion)
    Object.defineProperty(performance, 'now', { configurable: true, value: () => 1000 })
  localStorage.clear()
  window.Date = new Proxy(Date, {
    construct: (target, args) => Reflect.construct(target, args.length ? args : [FIXED_TIME]),
    apply: () => new Date(FIXED_TIME).toString(),
    get: (target, key) => (key === 'now' ? () => FIXED_TIME : Reflect.get(target, key)),
  })
  const originalError = console.error.bind(console)
  console.error = (...args) => {
    errors.push(args.map(String).join(' '))
    originalError(...args)
  }
  window.addEventListener('error', (event) => errors.push(event.message))
  window.addEventListener('unhandledrejection', (event) => errors.push(String(event.reason)))
  window.open = () => {
    throw new Error('External navigation blocked in capture')
  }
  window.fetch = async (input) => {
    const url = String(input instanceof Request ? input.url : input)
    if (url.endsWith('/content.json'))
      return new Response('{}', { headers: { 'Content-Type': 'application/json' } })
    throw new Error(`Capture fixture: unexpected HTTP ${url}`)
  }
  ;(
    window as unknown as { __BLOCKFIELD_CAPTURE_INVOKE__: typeof invoke }
  ).__BLOCKFIELD_CAPTURE_INVOKE__ = (async (command, args) => {
    const payload = (args ?? {}) as Record<string, unknown>
    if (command === 'plugin:event|listen') {
      events.set(Number(payload.handler), String(payload.event))
      return Number(payload.handler)
    }
    if (command === 'plugin:event|unlisten') {
      events.delete(Number(payload.eventId))
      return
    }
    if (command.startsWith('plugin:window|')) return nativeInvoke(command, args)
    if (command === 'preview_skin') return { skin, cape, slim: false, source: 'custom' }
    if (command === 'download_modpack') {
      emit('launcher://status', {
        phase: 'modpack',
        message: 'Downloading fixture pack',
        cancelable: true,
      })
      emit('download://progress', {
        filePath: 'mods/capture-fixture.jar',
        fileIndex: 42,
        fileCount: 100,
        bytesDownloaded: 420_000_000,
        fileBytesTotal: 1_000_000_000,
        totalBytesDownloaded: 420_000_000,
        totalBytesAll: 1_000_000_000,
        speedBytesPerSec: 12_000_000,
      })
      return new Promise(() => {})
    }
    return fixtureResponse(selection.state, command)
  }) as typeof invoke
  document.documentElement.dataset.captureStatic = String(!selection.motion)
}

async function until(check: () => boolean, label: string) {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (check()) return
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
  throw new Error(`Capture timeout: ${label}`)
}

function click(text: string, selector = 'button') {
  const button = [...document.querySelectorAll<HTMLButtonElement>(selector)].find(
    (node) => node.textContent?.trim() === text,
  )
  if (!button) return false
  button.click()
  return true
}

export async function drive() {
  try {
    const { state } = selection
    if (state.startsWith('settings-')) {
      await until(() => click('НАСТРОЙКИ', 'nav button'), 'settings navigation')
      const tabs: Record<string, string> = {
        general: 'ОБЩИЕ',
        runtime: 'JAVA И ПАМЯТЬ',
        commands: 'КОМАНДЫ',
        skin: 'СКИН',
        launcher: 'ЛАУНЧЕР',
      }
      await until(() => click(tabs[state.slice(9)]!), 'settings tab')
    } else if (state === 'stats') {
      await until(() => click('ТОП', 'nav button'), 'stats navigation')
    } else if (state === 'update-42') {
      await until(() => click('ОБНОВЛЕНИЯ', 'nav button'), 'update navigation')
      await until(() => /42(?:\.0)?\s*%/.test(document.body.textContent ?? ''), '42% download')
    } else {
      await until(() => !document.querySelector('#root:empty'), 'root render')
    }
    await document.fonts.load('400 16px Inter', 'Capture Привіт')
    await document.fonts.load('400 16px "IBM Plex Mono"', 'Capture Привіт')
    await document.fonts.ready
    await new Promise((resolve) => setTimeout(resolve, 1200))
    const faces = [...document.fonts].map((face) => ({
      family: face.family,
      weight: face.weight,
      status: face.status,
      unicodeRange: face.unicodeRange,
    }))
    if (
      !faces.some(
        (face) => face.family.replace(/["']/g, '') === 'Inter' && face.status === 'loaded',
      )
    )
      throw new Error('Inter font was not loaded: invalid baseline')
    let webgl: Record<string, unknown> | null = null
    if (state === 'settings-skin') {
      const canvas = document.querySelector<HTMLCanvasElement>('canvas[aria-label]')
      const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl')
      if (!gl) throw new Error('Skin WebGL context unavailable: invalid skin baseline')
      const debug = gl.getExtension('WEBGL_debug_renderer_info')
      webgl = {
        renderer: gl.getParameter(debug?.UNMASKED_RENDERER_WEBGL ?? gl.RENDERER),
        vendor: gl.getParameter(debug?.UNMASKED_VENDOR_WEBGL ?? gl.VENDOR),
        version: gl.getParameter(gl.VERSION),
      }
    }
    await nativeInvoke('save_settings', {
      config: {
        ...nativeConfig,
        lang:
          'capture-ready:' +
          JSON.stringify({
            state,
            requestedLanguage: selection.lang,
            effectiveLanguage: 'ru',
            scale: selection.scale,
            motion: selection.motion,
            viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
            fonts: {
              inter: document.fonts.check('400 16px Inter', 'Capture Привіт'),
              ibmPlexMono: document.fonts.check('400 16px "IBM Plex Mono"', 'Capture Привіт'),
              body: getComputedStyle(document.body).fontFamily,
              faces,
            },
            renderedFontFamilies: [
              ...new Set(
                [...document.querySelectorAll('body *')]
                  .filter((node) => node.textContent?.trim())
                  .map((node) => getComputedStyle(node).fontFamily),
              ),
            ],
            webgl,
            errors,
          }),
      },
    })
  } catch (error) {
    await nativeInvoke('save_settings', {
      config: { ...nativeConfig, lang: 'capture-failed:' + String(error) },
    })
  }
}
