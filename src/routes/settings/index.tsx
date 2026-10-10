import { createFileRoute } from '@tanstack/react-router'
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useRef, useState } from 'react'
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
import { useTheme } from '@/hooks/useTheme'
import { type CoreMonitor, listCoreMonitors } from '@/lib/coreTools'
import { extractErrorMessage } from '@/lib/extractError'

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
        loadError: `設定の読み込みに失敗しました: ${extractErrorMessage(e)}`,
      }
    }
  },
  pendingComponent: SettingsPending,
  component: SettingsPage,
})

const fpsOptions = [10, 15, 24, 30, 60]

function SettingsPending() {
  return (
    <p className="py-8 text-center text-muted-foreground text-sm">
      読み込み中...
    </p>
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
    setProjectorMonitorIndex,
    browsePath,
    browseMatchingSourcePath,
    browseMatchingTargetPath,
    enableDeveloperMode,
    disableDeveloperMode,
  } = useSettings({ settings: initialSettings, cameras, loadError })
  const { handleLabelClick } = useDeveloperMode(
    settings.developerMode,
    enableDeveloperMode,
  )
  const { recalibrate, recalStatus } = useRecalibrate()
  const { theme, setTheme } = useTheme()
  const [monitors, setMonitors] = useState<CoreMonitor[]>([])
  const [monitorsLoading, setMonitorsLoading] = useState(false)
  const [monitorError, setMonitorError] = useState<string | null>(null)
  const monitorRequest = useRef(0)

  const refreshMonitors = useCallback(async () => {
    const request = ++monitorRequest.current
    setMonitorsLoading(true)
    setMonitorError(null)
    try {
      const available = await listCoreMonitors()
      if (request !== monitorRequest.current) return
      setMonitors(available)
      setMonitorsLoading(false)
    } catch (error) {
      if (request !== monitorRequest.current) return
      setMonitorError(
        `モニター一覧を取得できませんでした: ${extractErrorMessage(error)}`,
      )
      setMonitorsLoading(false)
    }
  }, [])

  useEffect(() => {
    if (!settings.developerMode) return
    void refreshMonitors()
    return () => {
      monitorRequest.current++
    }
  }, [settings.developerMode, refreshMonitors])

  return (
    <>
      <StatusBanner status={status} />

      <div className="space-y-8">
        <SectionPanel heading="表示設定">
          <FormField>
            <FormField.Label htmlFor="theme">テーマ</FormField.Label>
            <FormField.Select
              className="sm:max-w-40"
              id="theme"
              value={theme}
              onChange={(e) => {
                const value = e.currentTarget.value
                if (
                  value === 'light' ||
                  value === 'dark' ||
                  value === 'system'
                ) {
                  setTheme(value)
                }
              }}
            >
              <FormField.Select.Option value="light">
                ライト
              </FormField.Select.Option>
              <FormField.Select.Option value="dark">
                ダーク
              </FormField.Select.Option>
              <FormField.Select.Option value="system">
                システム
              </FormField.Select.Option>
            </FormField.Select>
          </FormField>
        </SectionPanel>

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

        <div className="flex justify-center">
          <Button disabled={status.type === 'loading'} onClick={() => save()}>
            保存
          </Button>
        </div>

        {settings.developerMode && (
          <SectionPanel heading="開発者設定">
            <div className="space-y-5">
              <FormField labelWidth="200px">
                <FormField.Label htmlFor="projector-monitor">
                  プロジェクターのモニター
                </FormField.Label>
                <FormField.Control className="space-y-2">
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <FormField.Select
                      id="projector-monitor"
                      className="min-w-0 sm:flex-1"
                      aria-describedby="projector-monitor-help"
                      value={settings.projectorMonitorIndex ?? ''}
                      disabled={monitorsLoading || monitorError !== null}
                      onChange={(e) => {
                        const value = e.currentTarget.value
                        setProjectorMonitorIndex(
                          value === '' ? undefined : Number(value),
                        )
                      }}
                    >
                      <FormField.Select.Option value="">
                        プロジェクターのモニターを選択
                      </FormField.Select.Option>
                      {settings.projectorMonitorIndex !== undefined &&
                        !monitors.some(
                          (monitor) =>
                            monitor.monitor_index ===
                            settings.projectorMonitorIndex,
                        ) && (
                          <FormField.Select.Option
                            value={settings.projectorMonitorIndex}
                            disabled
                          >
                            保存済みモニター（番号{' '}
                            {settings.projectorMonitorIndex}
                            ・現在取得できません）
                          </FormField.Select.Option>
                        )}
                      {monitors.map((monitor) => (
                        <FormField.Select.Option
                          key={monitor.monitor_index}
                          value={monitor.monitor_index}
                        >
                          {monitor.name}（{monitor.width} × {monitor.height}）
                          {monitor.primary ? '・メイン' : ''}
                        </FormField.Select.Option>
                      ))}
                    </FormField.Select>
                    <Button
                      variant="secondary"
                      className="shrink-0"
                      disabled={monitorsLoading}
                      onClick={() => void refreshMonitors()}
                    >
                      再取得
                    </Button>
                  </div>
                  <p
                    id="projector-monitor-help"
                    role="status"
                    className="break-words text-muted-foreground text-xs"
                  >
                    {monitorsLoading
                      ? 'モニター一覧を取得中...'
                      : (monitorError ??
                        (monitors.length === 0
                          ? 'モニターが見つかりません。接続を確認して再取得してください。'
                          : '投影先を選択して保存してください。自動では選択しません。'))}
                  </p>
                </FormField.Control>
              </FormField>

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

              <FormField labelWidth="200px">
                <FormField.Label htmlFor="matching-source-path">
                  マッチング元 PLY（固定）
                </FormField.Label>
                <FormField.Control className="flex gap-2">
                  <TextInput
                    id="matching-source-path"
                    readOnly
                    type="text"
                    value={settings.matchingSourcePath ?? ''}
                  />
                  <Button
                    variant="secondary"
                    onClick={browseMatchingSourcePath}
                  >
                    参照...
                  </Button>
                </FormField.Control>
              </FormField>

              <FormField labelWidth="200px">
                <FormField.Label htmlFor="matching-target-path">
                  スキャン先 PLY 保存パス
                </FormField.Label>
                <FormField.Control className="space-y-2">
                  <div className="flex gap-2">
                    <TextInput
                      id="matching-target-path"
                      aria-describedby="matching-target-help"
                      readOnly
                      type="text"
                      value={settings.matchingTargetPath ?? ''}
                    />
                    <Button
                      variant="secondary"
                      className="shrink-0"
                      onClick={browseMatchingTargetPath}
                    >
                      保存先...
                    </Button>
                  </div>
                  <p
                    id="matching-target-help"
                    className="text-muted-foreground text-xs"
                  >
                    スキャンするたびに上書きし、マッチング先として使います。新しい
                    PLY ファイルの保存先も指定できます。
                  </p>
                </FormField.Control>
              </FormField>

              <FormField labelWidth="200px">
                <FormField.Label htmlFor="matching-mode">
                  マッチングモード
                </FormField.Label>
                <FormField.Select
                  className="sm:max-w-40"
                  id="matching-mode"
                  value={settings.matchingMode ?? 'matching'}
                  onChange={(e) => {
                    updateField('matchingMode', e.currentTarget.value)
                  }}
                >
                  <FormField.Select.Option value="matching">
                    マッチング
                  </FormField.Select.Option>
                  <FormField.Select.Option value="ransac">
                    RANSAC
                  </FormField.Select.Option>
                  <FormField.Select.Option value="icp">
                    ICP
                  </FormField.Select.Option>
                </FormField.Select>
              </FormField>

              <FormField labelWidth="200px">
                <FormField.Label htmlFor="matching-voxel-size">
                  ボクセルサイズ
                </FormField.Label>
                <TextInput
                  className="sm:max-w-40"
                  id="matching-voxel-size"
                  min={0}
                  step={0.01}
                  type="number"
                  value={settings.matchingVoxelSize ?? ''}
                  onChange={(e) => {
                    const value = Number.parseFloat(e.currentTarget.value)
                    updateField(
                      'matchingVoxelSize',
                      Number.isNaN(value) ? 0 : value,
                    )
                  }}
                />
              </FormField>

              <FormField labelWidth="200px">
                <FormField.Label htmlFor="matching-ransac-iterations">
                  RANSAC 反復回数
                </FormField.Label>
                <TextInput
                  className="sm:max-w-40"
                  id="matching-ransac-iterations"
                  min={0}
                  step={1}
                  type="number"
                  value={settings.matchingRansacIterations ?? ''}
                  onChange={(e) => {
                    const value = Number.parseInt(e.currentTarget.value, 10)
                    updateField(
                      'matchingRansacIterations',
                      Number.isNaN(value) ? 0 : value,
                    )
                  }}
                />
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

              <div className="flex justify-center">
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
          className="select-none pt-8 text-center text-muted-foreground text-xs"
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
