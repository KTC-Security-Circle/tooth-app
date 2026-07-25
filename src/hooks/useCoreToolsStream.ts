import { useCallback, useEffect, useRef, useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import {
  type CoreToolsResponse,
  onCoreToolsEvent,
  onCoreToolsResponse,
  sendCoreToolsCommand,
  startCoreTools,
  stopCoreTools,
} from '@/lib/coreTools'
import { extractErrorMessage } from '@/lib/extractError'

export type StreamRole = 'left' | 'right'

export type StreamState = 'idle' | 'starting' | 'streaming' | 'stopping'

type CameraOpenState = {
  left: boolean
  right: boolean
}

type StreamActiveState = {
  left: boolean
  right: boolean
}

/**
 * core-tools (tooth-backend) のライフサイクルとカメラ/ストリーム操作を管理する hook。
 *
 * - アプリ起動時に core-tools を自動起動 (ready 待ち)
 * - カメラ open/close、stream start/stop のコマンド送信
 * - `core-tools:response` / `core-tools:event` を購読し状態を反映
 * - アンマウント時に core-tools を停止
 */
export function useCoreToolsStream() {
  const [status, setStatus] = useState<Status>({ type: 'idle' })
  const [streamState, setStreamState] = useState<StreamState>('idle')
  const [cameraOpen, setCameraOpen] = useState<CameraOpenState>({
    left: false,
    right: false,
  })
  const [streamActive, setStreamActive] = useState<StreamActiveState>({
    left: false,
    right: false,
  })

  // コマンド id 採番用
  const idCounter = useRef(0)
  const pendingCommands = useRef<
    Map<
      string,
      { resolve: (r: CoreToolsResponse) => void; reject: (e: unknown) => void }
    >
  >(new Map())

  // core-tools 起動 (マウント時1回)
  useEffect(() => {
    let cancelled = false
    setStatus({ type: 'loading', message: '処理エンジンを起動中...' })

    startCoreTools()
      .then(() => {
        if (!cancelled) {
          setStatus({ type: 'success', message: '処理エンジン起動完了' })
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setStatus({
            type: 'error',
            message: `処理エンジンの起動に失敗しました: ${extractErrorMessage(e)}`,
          })
        }
      })

    return () => {
      cancelled = true
      stopCoreTools().catch((e) => {
        logWarn('stop_core_tools failed on unmount', e)
      })
    }
  }, [])

  // response / event 購読 (マウント時1回)
  useEffect(() => {
    const unsubs: Array<() => void> = []

    onCoreToolsResponse((response) => {
      const id = response.id
      if (id !== undefined) {
        const pending = pendingCommands.current.get(id)
        if (pending) {
          pendingCommands.current.delete(id)
          pending.resolve(response)
        }
      }
    }).then((unsub) => unsubs.push(unsub))

    onCoreToolsEvent((event) => {
      switch (event.event) {
        case 'camera_opened': {
          const role = event.role as StreamRole | undefined
          if (role === 'left' || role === 'right') {
            setCameraOpen((prev) => ({ ...prev, [role]: true }))
          }
          break
        }
        case 'camera_closed': {
          const role = event.role as StreamRole | undefined
          if (role === 'left' || role === 'right') {
            setCameraOpen((prev) => ({ ...prev, [role]: false }))
            setStreamActive((prev) => ({ ...prev, [role]: false }))
          }
          break
        }
        case 'stream_started': {
          const role = event.role as StreamRole | undefined
          if (role === 'left' || role === 'right') {
            setStreamActive((prev) => ({ ...prev, [role]: true }))
          }
          break
        }
        case 'stream_stopped': {
          const role = event.role as StreamRole | undefined
          if (role === 'left' || role === 'right') {
            setStreamActive((prev) => ({ ...prev, [role]: false }))
          }
          break
        }
        default:
          break
      }
    }).then((unsub) => unsubs.push(unsub))

    return () => {
      for (const unsub of unsubs) {
        unsub()
      }
    }
  }, [])

  const sendCommand = useCallback(
    <T extends Record<string, unknown>>(
      cmd: string,
      extra: Record<string, unknown> = {},
    ): Promise<T> => {
      const id = String(++idCounter.current)
      const json = JSON.stringify({ id, cmd, ...extra })
      return new Promise<T>((resolve, reject) => {
        pendingCommands.current.set(id, {
          resolve: (r) => resolve(r as unknown as T),
          reject,
        })
        sendCoreToolsCommand(json).catch((e) => {
          pendingCommands.current.delete(id)
          reject(e)
        })
      })
    },
    [],
  )

  const openCamera = useCallback(
    (role: StreamRole, cameraId: number) => {
      return sendCommand<{ ok: boolean }>('open_camera', {
        camera_id: cameraId,
        role,
      })
    },
    [sendCommand],
  )

  const startStream = useCallback(
    (role: StreamRole) => {
      return sendCommand<{ ok: boolean; url: string }>('start_stream', { role })
    },
    [sendCommand],
  )

  const stopStream = useCallback(
    (role: StreamRole) => {
      return sendCommand<{ ok: boolean }>('stop_stream', { role })
    },
    [sendCommand],
  )

  /**
   * 両眼ストリームを開始する。
   * 事前に両カメラが open されている必要がある。
   */
  const startStreams = useCallback(
    async (leftCameraId: number, rightCameraId: number) => {
      setStreamState('starting')
      setStatus({ type: 'loading', message: 'ストリームを開始しています...' })
      try {
        if (!cameraOpen.left) {
          await openCamera('left', leftCameraId)
        }
        if (!cameraOpen.right) {
          await openCamera('right', rightCameraId)
        }
        await Promise.all([startStream('left'), startStream('right')])
        setStreamState('streaming')
        setStatus({ type: 'success', message: 'ストリームを開始しました' })
      } catch (e) {
        setStreamState('idle')
        setStatus({
          type: 'error',
          message: `ストリーム開始に失敗しました: ${extractErrorMessage(e)}`,
        })
      }
    },
    [cameraOpen.left, cameraOpen.right, openCamera, startStream],
  )

  /**
   * 両眼ストリームを停止する。
   */
  const stopStreams = useCallback(async () => {
    setStreamState('stopping')
    setStatus({ type: 'loading', message: 'ストリームを停止しています...' })
    try {
      if (streamActive.left) {
        await stopStream('left')
      }
      if (streamActive.right) {
        await stopStream('right')
      }
      setStreamState('idle')
      setStatus({ type: 'success', message: 'ストリームを停止しました' })
    } catch (e) {
      setStreamState('streaming')
      setStatus({
        type: 'error',
        message: `ストリーム停止に失敗しました: ${extractErrorMessage(e)}`,
      })
    }
  }, [streamActive.left, streamActive.right, stopStream])

  return {
    status,
    streamState,
    cameraOpen,
    streamActive,
    startStreams,
    stopStreams,
  }
}

function logWarn(msg: string, e: unknown) {
  // eslint-disable-next-line no-console
  console.warn(msg, e)
}
