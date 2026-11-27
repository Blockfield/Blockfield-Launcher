import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

const script = fileURLToPath(new URL('./validate-production-config.mjs', import.meta.url))
const run = (value, workshop = '') =>
  spawnSync(process.execPath, [script], {
    env: {
      ...process.env,
      VITE_BLOCKFIELD_PACK_URL: value ?? '',
      VITE_BLOCKFIELD_WORKSHOP_PACK_URL: workshop,
    },
    encoding: 'utf8',
  })

test('accepts the configured production origin', () => {
  assert.equal(run('https://blockfield.pro').status, 0)
})

test('rejects missing, insecure, private, and mismatching origins', () => {
  for (const value of [
    '',
    'http://blockfield.pro',
    'https://localhost/pack',
    'https://10.0.0.1/pack',
    'https://other.example.com',
  ])
    assert.notEqual(run(value).status, 0, value)
})

test('accepts an empty or valid optional workshop pack URL', () => {
  assert.equal(run('https://blockfield.pro', '').status, 0)
  assert.equal(run('https://blockfield.pro', 'https://blockfield.pro/workshop').status, 0)
})

test('rejects an invalid workshop pack URL', () => {
  for (const workshop of ['http://blockfield.pro/workshop', 'https://10.0.0.1/workshop'])
    assert.notEqual(run('https://blockfield.pro', workshop).status, 0, workshop)
})
