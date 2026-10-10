import { createFileRoute } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'

const SCAN_POSITIONS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]

import PageHeader from '@/components/app/PageHeader'
import SectionPanel from '@/components/app/SectionPanel'
import Button from '@/components/ui/Button'
import type { Status } from '@/components/ui/StatusBanner'
import StatusBanner from '@/components/ui/StatusBanner'
import { extractErrorMessage } from '@/lib/extractError'

export const Route = createFileRoute('/')({
  component: Index,
})

function Index() {
  const [running, setRunning] = useState(false)
  const [status, setStatus] = useState<Status>({ type: 'idle' })
  const [rotationCount, setRotationCount] = useState(0)
  const [rotationInProgress, setRotationInProgress] = useState(false)
  const [scanStatus, setScanStatus] = useState<Array<string>>(() =>
    Array.from({ length: 12 }, () => '未開始'),
  )
  const listenerReady = useRef<Promise<void> | null>(null)

  useEffect(() => {
    let active = true
    let unsubscribe: (() => void) | undefined

    const registration = listen<{
      positionIndex: number
      totalPositions: number
      phase: string
      message?: string
    }>('live-scan:status', (event) => {
      if (!active) return
      const { positionIndex, totalPositions, phase, message } = event.payload
      if (phase === 'rotating') {
        setRotationInProgress(true)
      } else {
        setRotationInProgress(false)
      }
      if (phase === 'preparing' && positionIndex > 0) {
        setRotationCount(Math.min(positionIndex, 11))
      }
      if (positionIndex >= 0 && positionIndex < 12) {
        setScanStatus((current) => {
          const next = [...current]
          next[positionIndex] = statusLabel(phase, message)
          return next
        })
      }
      if (phase === 'failed') {
        setRunning(false)
        setStatus({
          type: 'error',
          message: message ?? 'スキャンに失敗しました',
        })
      } else if (
        phase === 'completed' &&
        totalPositions === 12 &&
        positionIndex === 11
      ) {
        setRunning(false)
        setStatus({
          type: 'success',
          message: '12方向のスキャンが完了しました',
        })
      } else {
        setStatus({
          type: 'loading',
          message: `${positionIndex + 1} / ${totalPositions}方向: ${statusLabel(phase, message)}`,
        })
      }
    }).then((remove) => {
      if (active) unsubscribe = remove
      else remove()
    })
    listenerReady.current = registration.then(() => undefined)

    return () => {
      active = false
      unsubscribe?.()
    }
  }, [])

  const startScan = async () => {
    const registration = listenerReady.current
    if (!registration) {
      setStatus({
        type: 'error',
        message: 'スキャンの準備中です。少し待ってください',
      })
      return
    }
    setRunning(true)
    setStatus({ type: 'loading', message: 'スキャンを準備しています...' })
    setScanStatus(Array.from({ length: 12 }, () => '未開始'))
    setRotationCount(0)
    setRotationInProgress(false)
    try {
      await registration
      await invoke('start_live_scan')
    } catch (error) {
      setRunning(false)
      setStatus({
        type: 'error',
        message: `スキャン開始に失敗しました: ${extractErrorMessage(error)} 設定画面でモニターを選択してください。`,
      })
    }
  }

  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="Tooth Calibrator"
        subtitle="ステレオカメラのライブスキャンを実行します。"
      />

      <StatusBanner status={status} />

      <SectionPanel heading="ライブスキャン">
        <div className="space-y-6">
          <p className="text-muted-foreground text-sm">
            スキャンモニターは設定画面で選択してください。
          </p>
          <Button disabled={running} onClick={startScan}>
            {running ? 'スキャン中...' : 'スキャン開始'}
          </Button>
          <ol className="grid grid-cols-1 gap-2 md:grid-cols-2">
            {SCAN_POSITIONS.map((position) => (
              <li
                key={position}
                className="rounded-md border border-border px-3 py-2 text-sm"
              >
                <span className="font-medium">{position + 1}方向</span>
                <span className="ml-3 text-muted-foreground">
                  {scanStatus[position]}
                </span>
              </li>
            ))}
          </ol>
          <p className="text-muted-foreground text-sm">
            回転: {rotationCount} / 11回
            {rotationInProgress ? '（回転中）' : ''}
          </p>
        </div>
      </SectionPanel>
    </div>
  )
}

function phaseLabel(phase: string): string {
  const labels: Record<string, string> = {
    preparing: '準備中',
    scanning: '撮影中',
    decoding: 'デコード中',
    reconstructing: '再構成中',
    matching: 'マッチング中',
    rotating: '回転中',
    completed: '完了',
    failed: '失敗',
  }
  return labels[phase] ?? phase
}

function statusLabel(phase: string, message?: string): string {
  if (message === 'position completed') return '完了'
  return message ?? phaseLabel(phase)
}
