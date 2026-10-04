import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const tauri = new URL('../src-tauri/', import.meta.url)
const config = JSON.parse(readFileSync(new URL('tauri.conf.json', tauri), 'utf8'))
const nsis = config.bundle.windows.nsis
const template = readFileSync(new URL(nsis.template, tauri), 'utf8')
const hooks = readFileSync(new URL(nsis.installerHooks, tauri), 'utf8')

test('installer branding uses native icons and 24-bit NSIS bitmaps', () => {
  for (const [name, width, height] of [
    ['headerImage', 150, 57],
    ['sidebarImage', 164, 314],
  ]) {
    const bmp = readFileSync(new URL(nsis[name], tauri))
    assert.equal(bmp.subarray(0, 2).toString(), 'BM')
    assert.equal(bmp.readInt32LE(18), width)
    assert.equal(bmp.readInt32LE(22), height)
    assert.equal(bmp.readUInt16LE(28), 24)
  }
  assert.deepEqual(nsis.languages, ['English', 'Russian', 'Ukrainian'])
  for (const name of ['installerIcon', 'uninstallerIcon']) {
    assert.equal(readFileSync(new URL(nsis[name], tauri)).readUInt16LE(2), 1)
  }
})

test('uninstall delegates to verified cleanup and never recursively erases AppData', () => {
  assert.doesNotMatch(template + hooks, /\bRmDir\s+\/r\b/i)
  assert.doesNotMatch(template, /DeleteAppDataCheckbox/)
  assert.match(hooks, /\$UpdateMode <> 1[\s\S]*ExecWait[^\n]*--uninstall-cleanup/)
  assert.match(hooks, /\$0 <> 0[\s\S]*Abort/)
  assert.match(template, /\$R0 <> 0[\s\S]*StrCpy \$R1 "\$R1 \/UPDATE"/)
})

test('signed NSIS plugins are registered before includes can invoke them', () => {
  assert.match(
    template,
    /{{#if signed_plugins_path}}\s*!addplugindir "{{signed_plugins_path}}"\s*{{\/if}}/,
  )
  assert.ok(
    template.indexOf('!addplugindir "{{signed_plugins_path}}"') < template.indexOf('\n!include '),
  )
})

test('Restart Manager checks the installed executable during install and cleanup', () => {
  assert.match(template, /^!include "Win\\RestartManager\.nsh"$/m)
  for (const [source, expectedCalls] of [
    [template, 2],
    [hooks, 1],
  ]) {
    const calls = [...source.matchAll(/^\s*!insertmacro CheckIfAppIsRunning (.+)$/gm)]
    assert.equal(calls.length, expectedCalls)
    for (const [, args] of calls) {
      assert.equal(args, '"$INSTDIR\\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"')
    }
  }
})
