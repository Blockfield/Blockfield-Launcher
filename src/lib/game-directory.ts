import { join } from '@tauri-apps/api/path'

export async function selectedGameDirectory(parent: string): Promise<string> {
  const name = parent
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop()
  return name?.toLowerCase() === 'blockfield' ? parent : join(parent, 'Blockfield')
}
