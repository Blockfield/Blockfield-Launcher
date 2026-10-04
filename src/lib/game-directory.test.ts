import { afterEach, expect, it, vi } from 'vitest'
import { posix, win32 } from 'node:path'
import { selectedGameDirectory } from './game-directory'

const { join } = vi.hoisted(() => ({ join: vi.fn() }))
vi.mock('@tauri-apps/api/path', () => ({ join }))
afterEach(() => join.mockReset())

it('adds Blockfield with the native separator, including roots and UNC paths', async () => {
  for (const [native, parent] of [
    [posix, '/home/player/Games'],
    [posix, '/'],
    [win32, 'D:\\Games'],
    [win32, 'D:\\'],
    [win32, '\\\\server\\share\\Games\\'],
  ] as const) {
    join.mockImplementation(native.join)
    expect(await selectedGameDirectory(parent)).toBe(native.join(parent, 'Blockfield'))
  }
})

it('uses an existing Blockfield folder without adding another one', async () => {
  for (const parent of ['/games/Blockfield', '/games/BlockField/', 'D:\\Games\\BLOCKFIELD\\']) {
    expect(await selectedGameDirectory(parent)).toBe(parent)
  }
  expect(join).not.toHaveBeenCalled()
})
