import { createFileRoute } from '@tanstack/react-router'
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import CameraSelectSection from '@/components/app/CameraSelectSection'
import SectionPanel from '@/components/app/SectionPanel'
import Button from '@/components/ui/Button'
import FormField from '@/components/ui/FormField'
import StatusBanner from '@/components/ui/StatusBanner'
import TextInput from '@/components/ui/TextInput'
import { useDeveloperMode } from '@/hooks/useDeveloperMode'
import { useRecalibrate } from '@/hooks/useRecalibrate'
import type { CameraInfo, Settings } from '@/hooks/useSettings'
import { defaultSettings, useSettings } from '@/hooks/useSettings'

export const Route = createFileRoute('/settings/')({
  loader: async () => {
    try {
      const [settings, cameras, version] = await Promise.all([
        invoke<Settings>('load_settings'),
        invoke<CameraInfo[]>('list_cameras'),
        getVersion(),
      ])
      return {
        settings,
        cameras,
        version,
        loadError: null,
      }
    } catch (e) {
      return {
        settings: defaultSettings,
        cameras: [] as CameraInfo[],
        version: '',
        loadError: `設定の読み込みに失敗しました: ${String(e)}`,
      }
    }
  },
  pendingComponent: SettingsPending,
  component: SettingsPage,
})

const fpsOptions = [10, 15, 24, 30, 60]

function SettingsPending() {
  return (
    <p className="py-8 text-center text-slate-500 text-sm">読み込み中...</p>
  )
}

function SettingsPage() {
  const {
    settings: initialSettings,
    cameras,
    version,
    loadError,
  } = Route.useLoaderData()
  const {
    settings,
    status,
    save,
    updateField,
    browsePath,
    enableDeveloperMode,
    disableDeveloperMode,
  } = useSettings({ settings: initialSettings, cameras, loadError })
  const { handleLabelClick } = useDeveloperMode(
    settings.developerMode,
    enableDeveloperMode,
  )
  const { recalibrate, recalStatus } = useRecalibrate()

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

        <SectionPanel heading="撮影設定">
          <FormField>
            <FormField.Label htmlFor="fps">FPS</FormField.Label>
            <FormField.Select
              className="sm:max-w-40"
              id="fps"
              value={settings.fps}
              onChange={(e) => {
                const value = Number(e.currentTarget.value)
                updateField('fps', value)
              }}
            >
              {fpsOptions.map((fps) => (
                <FormField.Select.Option key={fps} value={fps}>
                  {fps}
                </FormField.Select.Option>
              ))}
            </FormField.Select>
          </FormField>
        </SectionPanel>

        <div className="flex items-center gap-4">
          <Button disabled={status.type === 'loading'} onClick={() => save()}>
            保存
          </Button>
        </div>

        {settings.developerMode && (
          <SectionPanel heading="開発者設定">
            <div className="space-y-5">
              <FormField labelWidth="200px">
                <FormField.Label htmlFor="calibration-path">
                  キャリブレーション用画像保存パス
                </FormField.Label>
                <FormField.Control className="flex gap-2">
                  <TextInput
                    id="calibration-path"
                    readOnly
                    type="text"
                    value={settings.calibrationImagePath ?? ''}
                  />
                  <Button variant="secondary" onClick={browsePath}>
                    参照...
                  </Button>
                </FormField.Control>
              </FormField>

              <div className="flex items-center gap-4">
                <Button
                  variant="secondary"
                  disabled={recalStatus.type === 'loading'}
                  onClick={recalibrate}
                >
                  再キャリブレーション
                </Button>
                <StatusBanner status={recalStatus} />
              </div>

              <div className="flex items-center gap-4">
                <Button variant="secondary" onClick={disableDeveloperMode}>
                  開発者モードを無効化
                </Button>
              </div>
            </div>
          </SectionPanel>
        )}
      </div>

      {version && (
        <p
          className="select-none pt-8 text-center text-slate-400 text-xs"
          onClick={handleLabelClick}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              handleLabelClick()
            }
          }}
        >
          バージョン {version}
        </p>
      )}
    </>
  )
}
