import { invoke } from '@tauri-apps/api/core'

export function moveTurntable(angle: number): Promise<void> {
  if (!Number.isFinite(angle))
    return Promise.reject(new Error('angle must be finite'))
  return invoke<void>('move_turntable', { angle })
}
