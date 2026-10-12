import { useCallback, useEffect, useRef, useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import { resolveCameraSelection } from '@/lib/cameraSelection'
import {
  type CoreToolsResponse,
  coreToolsStatus,
  onCoreToolsEvent,
  onCoreToolsResponse,
  sendCoreToolsCommand,
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

export async function startStreamPair(
  startStream: (role: StreamRole) => Promise<CoreToolsResponse>,
  stopStream: (role: StreamRole) => Promise<CoreToolsResponse>,
): Promise<
  { ok: true } | { ok: false; active: StreamActiveState; message: string }
> {
  const starts = await Promise.allSettled([
    startStream('left'),
    startStream('right'),
  ])
  const failure = starts.find((result) => result.status === 'rejected')
  if (!failure) return { ok: true }

  // A rejected request may still have started a stream; stop both after both settle.
  const stops = await Promise.allSettled([
    stopStream('left'),
    stopStream('right'),
  ])
  const cleanupFailure = stops.find((result) => result.status === 'rejected')
  return {
    ok: false,
    active: {
      left: stops[0].status === 'rejected',
      right: stops[1].status === 'rejected',
    },
    message: `ストリーム開始に失敗しました: ${extractErrorMessage(failure.reason)}${
      cleanupFailure
        ? `。停止にも失敗しました: ${extractErrorMessage(cleanupFailure.reason)}。ストリーム停止を再試行してください。`
        : ''
    }`,
  }
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

  const settlePendingCommands = useCallback((reason: string) => {
    for (const [, pending] of pendingCommands.current) {
      pending.reject(new Error(reason))
    }
    pendingCommands.current.clear()
  }, [])

  // core-tools 状態ポーリング (マウント時 + 2s 間隔)。
  // 起動自体は Rust 側の setup hook で行われるため、ここでは状態反映のみ。
  useEffect(() => {
    let cancelled = false
    let timer: ReturnType<typeof setInterval> | undefined

    const poll = async () => {
      if (cancelled) return
      try {
        const s = await coreToolsStatus()
        if (cancelled) return
        if (s === 'running') {
          setStatus({ type: 'success', message: '処理エンジン起動完了' })
        } else if (s === 'failed') {
          setStatus({
            type: 'error',
            message: '処理エンジンの起動に失敗しました',
          })
          settlePendingCommands('core-tools failed')
        } else if (s === 'disconnected') {
          setStatus({
            type: 'error',
            message: '処理エンジンが切断されました',
          })
          settlePendingCommands('core-tools disconnected')
        } else {
          // idle / starting / ready → 起動中表示
          setStatus({ type: 'loading', message: '処理エンジンを起動中...' })
        }
      } catch (e) {
        if (!cancelled) {
          logWarn('core_tools_status poll failed', e)
        }
      }
    }

    void poll()
    timer = setInterval(poll, 2000)

    return () => {
      cancelled = true
      if (timer) clearInterval(timer)
    }
  }, [settlePendingCommands])

  // response / event 購読 (マウント時1回)
  useEffect(() => {
    const unsubs: Array<() => void> = []

    onCoreToolsResponse((response) => {
      const id = response.id
      if (id !== undefined) {
        const pending = pendingCommands.current.get(id)
        if (pending) {
          pendingCommands.current.delete(id)
          if (response.ok === false) {
            pending.reject(response)
          } else {
            pending.resolve(response)
          }
        }
      }
    }).then((unsub) => unsubs.push(unsub))

    onCoreToolsEvent((event) => {
      switch (event.event) {
        case 'ready': {
          setStatus({ type: 'success', message: '処理エンジン起動完了' })
          break
        }
        case 'camera_opened': {
          const role = event.role
          if (role === 'left' || role === 'right') {
            setCameraOpen((prev) => ({ ...prev, [role]: true }))
          }
          break
        }
        case 'camera_closed': {
          const role = event.role
          if (role === 'left' || role === 'right') {
            setCameraOpen((prev) => ({ ...prev, [role]: false }))
            setStreamActive((prev) => ({ ...prev, [role]: false }))
          }
          break
        }
        case 'stream_started': {
          const role = event.role
          if (role === 'left' || role === 'right') {
            setStreamActive((prev) => ({ ...prev, [role]: true }))
          }
          break
        }
        case 'stream_stopped': {
          const role = event.role
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
    (
      cmd: string,
      extra: Record<string, unknown> = {},
    ): Promise<CoreToolsResponse> => {
      const id = String(++idCounter.current)
      const json = JSON.stringify({ id, cmd, ...extra })
      return new Promise<CoreToolsResponse>((resolve, reject) => {
        pendingCommands.current.set(id, {
          resolve,
          reject,
        })
        const timer = setTimeout(() => {
          const entry = pendingCommands.current.get(id)
          if (entry) {
            pendingCommands.current.delete(id)
            reject(new Error(`Command "${cmd}" timed out after 10s`))
          }
        }, 10000)
        sendCoreToolsCommand(json).catch((e) => {
          clearTimeout(timer)
          pendingCommands.current.delete(id)
          reject(e)
        })
      })
    },
    [],
  )

  const openCamera = useCallback(
    (role: StreamRole, cameraId: number) => {
      return sendCommand('open_camera', {
        camera_id: cameraId,
        role,
      })
    },
    [sendCommand],
  )

  const startStream = useCallback(
    (role: StreamRole) => {
      return sendCommand('start_stream', { role })
    },
    [sendCommand],
  )

  const stopStream = useCallback(
    (role: StreamRole) => {
      return sendCommand('stop_stream', { role })
    },
    [sendCommand],
  )

  /**
   * 両眼ストリームを開始する。
   * 選択したカメラを開き直してから開始する。
   */
  const startStreams = useCallback(
    async (leftCameraId: number, rightCameraId: number) => {
      const selection = resolveCameraSelection(
        String(leftCameraId),
        String(rightCameraId),
      )
      if ('error' in selection) {
        setStatus({ type: 'error', message: selection.error })
        return
      }
      setStreamState('starting')
      setStatus({ type: 'loading', message: 'ストリームを開始しています...' })
      try {
        await openCamera('left', leftCameraId)
        await openCamera('right', rightCameraId)
        const result = await startStreamPair(startStream, stopStream)
        if (!result.ok) {
          setStreamActive(result.active)
          setStreamState(
            result.active.left || result.active.right ? 'streaming' : 'idle',
          )
          setStatus({ type: 'error', message: result.message })
          return
        }
        setStreamActive({ left: true, right: true })
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
    [openCamera, startStream, stopStream],
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
        setStreamActive((prev) => ({ ...prev, left: false }))
      }
      if (streamActive.right) {
        await stopStream('right')
        setStreamActive((prev) => ({ ...prev, right: false }))
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
