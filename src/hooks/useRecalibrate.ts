import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import { extractErrorMessage } from '@/lib/extractError'

export function useRecalibrate() {
  const [recalStatus, setRecalStatus] = useState<Status>({ type: 'idle' })

  const recalibrate = async () => {
    setRecalStatus({ type: 'loading', message: '処理中...' })
    try {
      await invoke<void>('recalibrate')
      setRecalStatus({
        type: 'success',
        message: '再キャリブレーションを実行しました',
      })
      window.setTimeout(() => {
        setRecalStatus({ type: 'idle' })
      }, 2000)
    } catch (e) {
      setRecalStatus({
        type: 'error',
        message: `再キャリブレーションに失敗しました: ${extractErrorMessage(e)}`,
      })
    }
  }

  return { recalibrate, recalStatus }
}
