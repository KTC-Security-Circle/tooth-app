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
}

export function useSettings({
  settings: initialSettings,
  cameras: initialCameras,
  loadError = null,
}: UseSettingsArgs) {
  const [settings, setSettings] = useState<Settings>(initialSettings)
  const [cameras] = useState<CameraInfo[]>(initialCameras)
  const [status, setStatus] = useState<Status>(
    loadError ? { type: 'error', message: loadError } : { type: 'idle' },
  )

  const save = async () => {
    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('save_settings', { settings })
      setStatus({ type: 'success', message: '設定を保存しました' })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `保存に失敗しました: ${String(e)}`,
      })
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
