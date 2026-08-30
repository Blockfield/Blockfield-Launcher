import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

const script = fileURLToPath(new URL('./validate-production-config.mjs', import.meta.url))
const run = (value) => spawnSync(process.execPath, [script], {
  env: { ...process.env, VITE_BLOCKFIELD_API_URL: value ?? '' },
  encoding: 'utf8',
})

test('accepts the configured production origin', () => {
  assert.equal(run('https://play.blockfield.gg/api/launcher/v1').status, 0)
})

test('rejects missing, insecure, private, and development origins', () => {
  for (const value of [
    '',
    'http://play.blockfield.gg/api',
    'https://localhost/api',
    'https://10.0.0.1/api',
    'https://backend.dev.example.com/api',
  ]) assert.notEqual(run(value).status, 0, value)
})
