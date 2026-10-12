import { invoke } from '@tauri-apps/api/core'
import { open, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import { resolveCameraSelection } from '@/lib/cameraSelection'
import { extractErrorMessage } from '@/lib/extractError'

export type Settings = {
  monitorIndex?: number | null
  cameraLeft: string
  cameraRight: string
  fps: number
  developerMode: boolean
  calibrationImagePath?: string
  matchingSourcePath?: string
  matchingTargetPath?: string
  matchingVoxelSize?: number
  matchingRansacIterations?: number
  projectorMonitorIndex?: number
  turntablePort: string
  turntableSpeed: number
  turntableAcceleration: number
  turntableSettleTimeMs: number
  turntableMoveTimeoutMs: number
}

export type SettingsPatch = Omit<Partial<Settings>, 'projectorMonitorIndex'> & {
  projectorMonitorIndex?: number | null
}

export type CameraInfo = {
  name: string
  path: string
}

export const defaultSettings: Settings = {
  monitorIndex: null,
  cameraLeft: '',
  cameraRight: '',
  fps: 30,
  developerMode: false,
  matchingSourcePath: '',
  matchingTargetPath: '',
  matchingVoxelSize: 0.25,
  matchingRansacIterations: 30,
  turntablePort: '',
  turntableSpeed: 0,
  turntableAcceleration: 0,
  turntableSettleTimeMs: 0,
  turntableMoveTimeoutMs: 0,
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
    const selection = resolveCameraSelection(
      settings.cameraLeft,
      settings.cameraRight,
    )
    if ('error' in selection) {
      setStatus({ type: 'error', message: selection.error })
      return false
    }
    if (
      !Number.isInteger(settings.fps) ||
      settings.fps < 1 ||
      settings.fps > 120
    ) {
      setStatus({
        type: 'error',
        message: 'FPS は 1〜120 の整数で指定してください。',
      })
      return false
    }

    setStatus({ type: 'loading', message: '保存中...' })
    try {
      const patch: SettingsPatch = {
        monitorIndex: settings.monitorIndex ?? null,
        cameraLeft: settings.cameraLeft,
        cameraRight: settings.cameraRight,
        fps: settings.fps,
      }
      if (settings.matchingSourcePath !== undefined) {
        patch.matchingSourcePath = settings.matchingSourcePath
      }
      if (settings.matchingTargetPath !== undefined) {
        patch.matchingTargetPath = settings.matchingTargetPath
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
    const selected = await saveDialog({
      filters: [{ name: 'PLY', extensions: ['ply'] }],
    })
    if (selected !== null && typeof selected === 'string') {
      setSettings((prev) => ({ ...prev, matchingTargetPath: selected }))
    }
  }

  const enableDeveloperMode = async () => {
    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('update_settings', { patch: { developerMode: true } })
      setSettings((prev) => ({ ...prev, developerMode: true }))
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
    setStatus({ type: 'loading', message: '保存中...' })
    try {
      await invoke<void>('update_settings', { patch: { developerMode: false } })
      setSettings((prev) => ({ ...prev, developerMode: false }))
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
    browseMatchingSourcePath,
    browseMatchingTargetPath,
    enableDeveloperMode,
    disableDeveloperMode,
  }
}
