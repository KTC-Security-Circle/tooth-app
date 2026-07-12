import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useState } from 'react'
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

const defaultSettings: Settings = {
  cameraLeft: '',
  cameraRight: '',
  fps: 30,
  calibrationImagePath: '',
  developerMode: false,
}

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(defaultSettings)
  const [cameras, setCameras] = useState<CameraInfo[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [status, setStatus] = useState<Status>({ type: 'idle' })

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      setIsLoading(true)
      setStatus({ type: 'idle' })

      try {
        const loadedSettings = await invoke<Settings>('load_settings')
        if (!cancelled) {
          setSettings(loadedSettings)
        }
      } catch (e) {
        if (!cancelled) {
          setStatus({
            type: 'error',
            message: `設定の読み込みに失敗しました: ${String(e)}`,
          })
        }
      }

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
    isLoading,
    status,
    save,
    updateField,
    browsePath,
    enableDeveloperMode,
    disableDeveloperMode,
  }
}
