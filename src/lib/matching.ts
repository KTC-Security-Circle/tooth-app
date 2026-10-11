import { invoke } from '@tauri-apps/api/core'
import type { Status } from '@/components/ui/StatusBanner'
import { extractErrorMessage } from '@/lib/extractError'

type MatchingResult = {
  fitness: number
  inlier_rmse: number
  transformation: number[][]
  correspondence_set: number[][]
}

// ページを移動しても実行状態を保持し、重複したリクエストを送らない。
let status: Status = { type: 'idle' }
const listeners = new Set<() => void>()

export function getMatchingStatus() {
  return status
}

export function subscribeMatching(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function updateStatus(next: Status) {
  status = next
  for (const listener of listeners) listener()
}

export async function runMatching() {
  if (status.type === 'loading') return

  updateStatus({
    type: 'loading',
    message: 'スキャンと3Dマッチングを実行中...',
  })
  try {
    const result = await invoke<MatchingResult>('run_matching')
    updateStatus({
      type: 'success',
      message: `3D マッチングが完了しました。適合率: ${result.fitness.toFixed(4)} / RMSE: ${result.inlier_rmse.toFixed(4)}`,
    })
  } catch (error) {
    updateStatus({
      type: 'error',
      message: `3D マッチングに失敗しました: ${extractErrorMessage(error)}`,
    })
  }
}
