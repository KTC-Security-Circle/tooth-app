import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'
import CameraSelectSection from '@/components/app/CameraSelectSection'
import PageHeader from '@/components/app/PageHeader'
import Button from '@/components/ui/Button'
import type { Status } from '@/components/ui/StatusBanner'
import StatusBanner from '@/components/ui/StatusBanner'
import type { CameraInfo } from '@/hooks/useSettings'

export const Route = createFileRoute('/setup/')({
  loader: async () => {
    try {
      const [cameras, calibrationPath] = await Promise.all([
        invoke<CameraInfo[]>('list_cameras'),
        invoke<string>('default_calibration_path'),
      ])
      return { cameras, calibrationPath, loadError: null as string | null }
    } catch (e) {
      return {
        cameras: [] as CameraInfo[],
        calibrationPath: '',
        loadError: `初期設定の読み込みに失敗しました: ${String(e)}`,
      }
    }
  },
  pendingComponent: SetupPending,
  component: Setup,
})

function SetupPending() {
  return (
    <p className="py-8 text-center text-slate-500 text-sm">読み込み中...</p>
  )
}

function Setup() {
  const navigate = useNavigate()
  const { cameras, calibrationPath, loadError } = Route.useLoaderData()

  const [cameraLeft, setCameraLeft] = useState('')
  const [cameraRight, setCameraRight] = useState('')
  const [status, setStatus] = useState<Status>(
    loadError ? { type: 'error', message: loadError } : { type: 'idle' },
  )

  const handleSave = async () => {
    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('save_settings', {
        settings: {
          cameraLeft,
          cameraRight,
          fps: 30,
          calibrationImagePath: calibrationPath,
          developerMode: false,
        },
      })
      await navigate({ to: '/' })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `保存に失敗しました: ${String(e)}`,
      })
    }
  }

  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="初期設定"
        subtitle="カメラの初回セットアップを行います。"
      />

      <StatusBanner status={status} />

      <div className="space-y-8">
        <CameraSelectSection
          cameras={cameras}
          cameraLeft={cameraLeft}
          cameraRight={cameraRight}
          onCameraLeftChange={(value) => {
            setCameraLeft(value)
          }}
          onCameraRightChange={(value) => {
            setCameraRight(value)
          }}
        />

        <div className="flex items-center gap-4">
          <Button disabled={status.type === 'loading'} onClick={handleSave}>
            完了
          </Button>
        </div>
      </div>
    </div>
  )
}
