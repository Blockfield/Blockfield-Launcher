import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

const script = fileURLToPath(new URL('./validate-production-config.mjs', import.meta.url))
const run = (value) => spawnSync(process.execPath, [script], {
  env: { ...process.env, VITE_BLOCKFIELD_PACK_URL: value ?? '' },
  encoding: 'utf8',
})

test('accepts the configured production origin', () => {
  assert.equal(run('https://modpack.nether.pp.ua').status, 0)
})

test('rejects missing, insecure, private, and mismatching origins', () => {
  for (const value of [
    '',
    'http://modpack.nether.pp.ua',
    'https://localhost/pack',
    'https://10.0.0.1/pack',
    'https://other.example.com',
  ]) assert.notEqual(run(value).status, 0, value)
})
