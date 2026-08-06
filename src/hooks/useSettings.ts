import { invoke } from '@tauri-apps/api/core'
import { dirname, join } from '@tauri-apps/api/path'
import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import { extractErrorMessage } from '@/lib/extractError'

export type Settings = {
  cameraLeft: string
  cameraRight: string
  fps: number
  developerMode: boolean
  calibrationImagePath?: string
  matchingSourcePath?: string
  matchingTargetPath?: string
  matchingMode?: string
  matchingVoxelSize?: number
  matchingRansacIterations?: number
}

export type SettingsPatch = Partial<Settings>

export type CameraInfo = {
  name: string
  path: string
}

export const defaultSettings: Settings = {
  cameraLeft: '',
  cameraRight: '',
  fps: 30,
  developerMode: false,
  matchingSourcePath: '',
  matchingTargetPath: '',
  matchingMode: 'matching',
  matchingVoxelSize: 0.25,
  matchingRansacIterations: 30,
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

  const save = async (): Promise<boolean> => {
    if (!settings.cameraLeft) {
      setStatus({ type: 'error', message: '左カメラを選択してください' })
      return false
    }
    if (!settings.cameraRight) {
      setStatus({ type: 'error', message: '右カメラを選択してください' })
      return false
    }

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      const patch: SettingsPatch = {
        cameraLeft: settings.cameraLeft,
        cameraRight: settings.cameraRight,
        fps: settings.fps,
      }
      if (settings.calibrationImagePath !== undefined) {
        patch.calibrationImagePath = settings.calibrationImagePath
      }
      if (settings.matchingSourcePath !== undefined) {
        patch.matchingSourcePath = settings.matchingSourcePath
      }
      if (settings.matchingTargetPath !== undefined) {
        patch.matchingTargetPath = settings.matchingTargetPath
      }
      if (settings.matchingMode !== undefined) {
        patch.matchingMode = settings.matchingMode
      }
      if (settings.matchingVoxelSize !== undefined) {
        patch.matchingVoxelSize = settings.matchingVoxelSize
      }
      if (settings.matchingRansacIterations !== undefined) {
        patch.matchingRansacIterations = settings.matchingRansacIterations
      }
      await invoke<void>('update_settings', { patch })
      setStatus({ type: 'success', message: '設定を保存しました' })
      return true
    } catch (e) {
      setStatus({
        type: 'error',
        message: `保存に失敗しました: ${extractErrorMessage(e)}`,
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
    if (selected !== null && typeof selected === 'string') {
      const parent = await dirname(selected)
      const defaultSourcePath = parent
        ? await join(parent, '3d_data', 'source.ply')
        : null
      const defaultTargetPath = parent
        ? await join(parent, '3d_data', 'target.ply')
        : null

      setSettings((prev) => {
        const next: Settings = { ...prev, calibrationImagePath: selected }
        if (!prev.matchingSourcePath && defaultSourcePath) {
          next.matchingSourcePath = defaultSourcePath
        }
        if (!prev.matchingTargetPath && defaultTargetPath) {
          next.matchingTargetPath = defaultTargetPath
        }
        return next
      })
    }
  }

  const browseMatchingSourcePath = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'PLY', extensions: ['ply'] }],
    })
    if (selected !== null && typeof selected === 'string') {
      setSettings((prev) => ({ ...prev, matchingSourcePath: selected }))
    }
  }

  const browseMatchingTargetPath = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'PLY', extensions: ['ply'] }],
    })
    if (selected !== null && typeof selected === 'string') {
      setSettings((prev) => ({ ...prev, matchingTargetPath: selected }))
    }
  }

  const enableDeveloperMode = async () => {
    setSettings((prev) => ({ ...prev, developerMode: true }))

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('update_settings', { patch: { developerMode: true } })
      const loaded = await invoke<Settings>('load_settings')
      setSettings(loaded)
      setStatus({
        type: 'success',
        message: '開発者モードを有効にしました',
      })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `開発者モードの保存に失敗しました: ${extractErrorMessage(e)}`,
      })
    }
  }

  const disableDeveloperMode = async () => {
    setSettings((prev) => ({ ...prev, developerMode: false }))

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('update_settings', { patch: { developerMode: false } })
      setSettings((prev) => ({
        ...prev,
        developerMode: false,
        calibrationImagePath: undefined,
        matchingSourcePath: undefined,
        matchingTargetPath: undefined,
        matchingMode: undefined,
        matchingVoxelSize: undefined,
        matchingRansacIterations: undefined,
      }))
      setStatus({
        type: 'success',
        message: '開発者モードを無効にしました',
      })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `開発者モードの保存に失敗しました: ${extractErrorMessage(e)}`,
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
    browseMatchingSourcePath,
    browseMatchingTargetPath,
    enableDeveloperMode,
    disableDeveloperMode,
  }
}
