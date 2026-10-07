import { beforeEach, expect, it, vi } from 'vitest'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

beforeEach(() => {
  vi.resetModules()
  invoke.mockReset()
  invoke.mockImplementation(async (command: string) => {
    if (command === 'load_settings') return { autoUpdate: true }
    if (command === 'check_modpack_version')
      return { needsUpdate: false, javaOk: true, loaderOk: true }
    if (command === 'game_status') return { phase: 'idle' }
    if (command === 'verify_files') return []
    throw new Error(command)
  })
})

it('shares the startup check and verifies files once across screen navigation', async () => {
  const { checkModpack } = await import('./modpack-check')
  await Promise.all([checkModpack(), checkModpack()])
  await checkModpack()
  expect(invoke.mock.calls.map(([command]) => command)).toEqual([
    'load_settings',
    'check_modpack_version',
    'game_status',
    'verify_files',
  ])
})

it('respects disabled startup verification but permits manual verification', async () => {
  invoke.mockImplementation(async (command: string) => {
    if (command === 'load_settings') return { autoUpdate: false }
    if (command === 'check_modpack_version')
      return { needsUpdate: false, javaOk: true, loaderOk: true }
    if (command === 'game_status') return { phase: 'idle' }
    return []
  })
  const { checkModpack } = await import('./modpack-check')
  await checkModpack()
  expect(invoke.mock.calls.some(([command]) => command === 'verify_files')).toBe(false)
  await checkModpack(true)
  expect(invoke).toHaveBeenCalledWith('verify_files', { profile: 'game' })
})

it('does not install a newer pack during the startup check', async () => {
  invoke.mockImplementation(async (command: string) =>
    command === 'load_settings'
      ? { autoUpdate: true }
      : { needsUpdate: true, javaOk: true, loaderOk: true },
  )
  const { checkModpack } = await import('./modpack-check')
  expect((await checkModpack()).needsUpdate).toBe(true)
  expect(invoke.mock.calls.some(([command]) => command === 'verify_files')).toBe(false)
  expect(invoke).not.toHaveBeenCalledWith('download_modpack')
})

it('does not cache failed verification and retries successfully', async () => {
  const { checkModpack } = await import('./modpack-check')
  const implementation = invoke.getMockImplementation()!
  invoke.mockImplementation(async (command: string) => {
    if (command === 'verify_files') throw new Error('corrupt file')
    return implementation(command)
  })
  await expect(checkModpack()).rejects.toThrow('corrupt file')
  invoke.mockImplementation(implementation)
  await expect(checkModpack()).resolves.toMatchObject({ needsUpdate: false })
})

it('rechecks after settings or an installation changes', async () => {
  const { checkModpack, invalidateModpackCheck } = await import('./modpack-check')
  await checkModpack()
  invalidateModpackCheck()
  await checkModpack()
  expect(invoke.mock.calls.filter(([command]) => command === 'check_modpack_version')).toHaveLength(
    2,
  )
})

it('discards a profile check invalidated during the native request without verifying another directory', async () => {
  const { checkModpack, invalidateModpackCheck, getModpackVersionSnapshot } =
    await import('./modpack-check')
  let finish!: (value: unknown) => void
  const implementation = invoke.getMockImplementation()!
  invoke.mockImplementation(async (command: string) => {
    if (command === 'check_modpack_version')
      return new Promise((resolve) => {
        finish = resolve
      })
    return implementation(command)
  })
  const oldCheck = checkModpack()
  const failed = expect(oldCheck).rejects.toThrow('Профиль изменён')
  await vi.waitFor(() => expect(finish).toBeTypeOf('function'))
  invalidateModpackCheck()
  invoke.mockImplementation(implementation)
  const current = await checkModpack()
  finish({ needsUpdate: false, javaOk: true, loaderOk: true, installedVersion: 'old-profile' })
  await failed
  expect(getModpackVersionSnapshot()).toBe(current)
  expect(invoke.mock.calls.filter(([command]) => command === 'verify_files')).toHaveLength(1)
})

it('does not verify files when the profile changes during the game-state request', async () => {
  const { checkModpack, invalidateModpackCheck } = await import('./modpack-check')
  let finish!: (value: unknown) => void
  const implementation = invoke.getMockImplementation()!
  invoke.mockImplementation(async (command: string) => {
    if (command === 'game_status')
      return new Promise((resolve) => {
        finish = resolve
      })
    return implementation(command)
  })
  const check = checkModpack()
  const failed = expect(check).rejects.toThrow('Профиль изменён')
  await vi.waitFor(() => expect(finish).toBeTypeOf('function'))
  invalidateModpackCheck()
  finish({ phase: 'idle' })
  await failed
  expect(invoke.mock.calls.some(([command]) => command === 'verify_files')).toBe(false)
})

it('binds verification to the workshop profile that was checked', async () => {
  const implementation = invoke.getMockImplementation()!
  invoke.mockImplementation(async (command: string) =>
    command === 'load_settings'
      ? { autoUpdate: true, activeProfile: 'workshop' }
      : implementation(command),
  )
  const { checkModpack } = await import('./modpack-check')
  await checkModpack()
  expect(invoke).toHaveBeenCalledWith('verify_files', { profile: 'workshop' })
})
