import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import CameraSelectSection from '@/components/app/CameraSelectSection'
import Button from '@/components/ui/Button'
import StatusBanner from '@/components/ui/StatusBanner'
import type { CameraInfo } from '@/hooks/useSettings'
import { useSettings } from '@/hooks/useSettings'
import { extractErrorMessage } from '@/lib/extractError'

export const Route = createFileRoute('/setup/')({
  loader: async () => {
    try {
      const [cameras, calibrationPath] = await Promise.all([
        invoke<CameraInfo[]>('list_cameras'),
        invoke<string>('default_calibration_path'),
      ])
      return { cameras, calibrationPath, loadError: null }
    } catch (e) {
      return {
        cameras: [] as CameraInfo[],
        calibrationPath: '',
        loadError: `初期設定の読み込みに失敗しました: ${extractErrorMessage(e)}`,
      }
    }
  },
  pendingComponent: SetupPending,
  component: Setup,
})

function SetupPending() {
  return (
    <p className="py-8 text-center text-muted-foreground text-sm">
      読み込み中...
    </p>
  )
}

function Setup() {
  const navigate = useNavigate()
  const { cameras, calibrationPath, loadError } = Route.useLoaderData()
  const { settings, status, save, updateField } = useSettings({
    settings: {
      cameraLeft: '',
      cameraRight: '',
      fps: 30,
      developerMode: false,
      calibrationImagePath: calibrationPath,
      turntablePort: '',
      turntableSpeed: 0,
      turntableAcceleration: 0,
      turntableSettleTimeMs: 0,
      turntableMoveTimeoutMs: 0,
    },
    cameras,
    loadError,
  })

  const handleSave = async () => {
    if (await save()) {
      await navigate({ to: '/' })
    }
  }

  return (
    <>
      <StatusBanner status={status} />

      <div className="space-y-8">
        <CameraSelectSection
          cameras={cameras}
          cameraLeft={settings.cameraLeft}
          cameraRight={settings.cameraRight}
          onCameraLeftChange={(value) => {
            updateField('cameraLeft', value)
          }}
          onCameraRightChange={(value) => {
            updateField('cameraRight', value)
          }}
        />

        <div className="flex justify-center">
          <Button disabled={status.type === 'loading'} onClick={handleSave}>
            完了
          </Button>
        </div>
      </div>
    </>
  )
}
