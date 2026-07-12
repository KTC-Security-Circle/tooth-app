import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'

export type Settings = {
  cameraLeft: string
  cameraRight: string
  fps: number
  calibrationImagePath: string
  developerMode: boolean
}

export type CameraInfo = {
  name: string
  path: string
}

export const defaultSettings: Settings = {
  cameraLeft: '',
  cameraRight: '',
  fps: 30,
  calibrationImagePath: '',
  developerMode: false,
}

interface UseSettingsArgs {
  settings: Settings
  cameras: CameraInfo[]
  loadError?: string | null
  requireCalibrationPath?: boolean
}

export function useSettings({
  settings: initialSettings,
  cameras: initialCameras,
  loadError = null,
  requireCalibrationPath = false,
}: UseSettingsArgs) {
  const [settings, setSettings] = useState<Settings>(initialSettings)
  const [cameras] = useState<CameraInfo[]>(initialCameras)
  const [status, setStatus] = useState<Status>(
    loadError ? { type: 'error', message: loadError } : { type: 'idle' },
  )

  const save = async (): Promise<boolean> => {
    if (!settings.cameraLeft) {
      setStatus({ type: 'error', message: '左カメラを選択してください' })
      return false
    }
    if (!settings.cameraRight) {
      setStatus({ type: 'error', message: '右カメラを選択してください' })
      return false
    }
    if (requireCalibrationPath && !settings.calibrationImagePath) {
      setStatus({
        type: 'error',
        message: 'キャリブレーション用画像保存パスを指定してください',
      })
      return false
    }

    try {
      await invoke<void>('validate_settings', { settings })
    } catch (e) {
      setStatus({
        type: 'error',
        message: String(e),
      })
      return false
    }

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('save_settings', { settings })
      setStatus({ type: 'success', message: '設定を保存しました' })
      return true
    } catch (e) {
      setStatus({
        type: 'error',
        message: `保存に失敗しました: ${String(e)}`,
      })
      return false
    }
  }

  const updateField = <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => {
    setSettings((prev) => ({ ...prev, [key]: value }))
  }

  const browsePath = async () => {
    const selected = await open({ directory: true })
    if (selected !== null) {
      setSettings((prev) => ({ ...prev, calibrationImagePath: selected }))
    }
  }

  const enableDeveloperMode = async () => {
    const nextSettings = { ...settings, developerMode: true }
    setSettings(nextSettings)

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('save_settings', { settings: nextSettings })
      setStatus({
        type: 'success',
        message: '開発者モードを有効にしました',
      })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `開発者モードの保存に失敗しました: ${String(e)}`,
      })
    }
  }

  const disableDeveloperMode = async () => {
    const nextSettings = { ...settings, developerMode: false }
    setSettings(nextSettings)

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('save_settings', { settings: nextSettings })
      setStatus({
        type: 'success',
        message: '開発者モードを無効にしました',
      })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `開発者モードの保存に失敗しました: ${String(e)}`,
      })
    }
  }

  return {
    settings,
    cameras,
    status,
    save,
    updateField,
    browsePath,
    enableDeveloperMode,
    disableDeveloperMode,
  }
}
