import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import CameraSelectSection from '@/components/app/CameraSelectSection'
import PageHeader from '@/components/app/PageHeader'
import Button from '@/components/ui/Button'
import type { Status } from '@/components/ui/StatusBanner'
import StatusBanner from '@/components/ui/StatusBanner'
import type { CameraInfo } from '@/hooks/useSettings'

export const Route = createFileRoute('/setup')({
  component: Setup,
})

function Setup() {
  const navigate = useNavigate()

  const [cameras, setCameras] = useState<CameraInfo[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [cameraLeft, setCameraLeft] = useState('')
  const [cameraRight, setCameraRight] = useState('')
  const [calibrationPath, setCalibrationPath] = useState('')
  const [status, setStatus] = useState<Status>({ type: 'idle' })

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      setIsLoading(true)
      try {
        const cameraList = await invoke<CameraInfo[]>('list_cameras')
        if (!cancelled) {
          setCameras(cameraList)
        }
      } catch (e) {
        if (!cancelled) {
          setStatus({
            type: 'error',
            message: `カメラ一覧の取得に失敗しました: ${String(e)}`,
          })
        }
      }

      try {
        const path = await invoke<string>('default_calibration_path')
        if (!cancelled) {
          setCalibrationPath(path)
        }
      } catch (e) {
        if (!cancelled) {
          setStatus({
            type: 'error',
            message: `デフォルト保存先の取得に失敗しました: ${String(e)}`,
          })
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false)
        }
      }
    }

    load()

    return () => {
      cancelled = true
    }
  }, [])

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

      {isLoading ? (
        <p className="py-8 text-center text-slate-500 text-sm">読み込み中...</p>
      ) : (
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
      )}
    </div>
  )
}
