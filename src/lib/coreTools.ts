import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * core-tools (tooth-backend) との連携ラッパ。
 *
 * Rust 側コマンド:
 * - `start_core_tools` : tooth-backend を起動し ready event を待つ
 * - `send_core_tools_command` : JSON Lines コマンドを stdin 経由で送信
 * - `stop_core_tools` : shutdown コマンド送信 → kill → Idle 遷移
 *
 * Rust 側イベント:
 * - `core-tools:response` : `{ id, ok, ... }` 形式のコマンド応答
 * - `core-tools:event`    : `{ event, ... }` 形式のイベント (ready / camera_opened / stream_started 等)
 */

export type CoreToolsResponse = {
  id?: string
  ok: boolean
  [key: string]: unknown
}

export type CoreToolsEvent = {
  event: string
  [key: string]: unknown
}

export type CoreMonitor = {
  monitor_index: number
  name: string
  width: number
  height: number
  primary: boolean
}

let monitorRequestSequence = 0

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function isCoreMonitor(value: unknown): value is CoreMonitor {
  return (
    isRecord(value) &&
    Number.isInteger(value.monitor_index) &&
    typeof value.name === 'string' &&
    Number.isInteger(value.width) &&
    Number.isInteger(value.height) &&
    typeof value.primary === 'boolean'
  )
}

/** core-tools の起動状態 (Rust 側 CoreToolsStatus enum と snake_case で対応) */
export type CoreToolsStatus =
  | 'idle'
  | 'starting'
  | 'ready'
  | 'running'
  | 'disconnected'
  | 'failed'

/** core-tools を起動する。ready event 受信 (10s タイムアウト) まで待つ。 */
export function startCoreTools(): Promise<void> {
  return invoke<void>('start_core_tools')
}

/** core-tools の現在の起動状態を取得する。 */
export function coreToolsStatus(): Promise<CoreToolsStatus> {
  return invoke<CoreToolsStatus>('core_tools_status')
}

/** core-tools へ JSON Lines コマンドを送信する。書き込み自体は Writer タスクが直列化する。 */
export function sendCoreToolsCommand(json: string): Promise<void> {
  return invoke<void>('send_core_tools_command', { json })
}

/** core-tools を停止する (graceful shutdown → kill)。 */
export function stopCoreTools(): Promise<void> {
  return invoke<void>('stop_core_tools')
}

/** `core-tools:response` イベントを購読する。アンサブスクライブ関数を返す。 */
export function onCoreToolsResponse(
  handler: (response: CoreToolsResponse) => void,
): Promise<() => void> {
  return listen<CoreToolsResponse>('core-tools:response', (event) => {
    handler(event.payload)
  })
}

/** `core-tools:event` イベントを購読する。アンサブスクライブ関数を返す。 */
export function onCoreToolsEvent(
  handler: (event: CoreToolsEvent) => void,
): Promise<() => void> {
  return listen<CoreToolsEvent>('core-tools:event', (event) => {
    handler(event.payload)
  })
}

/** core-tools の monitor 一覧を相関 response から取得する。 */
export async function listCoreMonitors(): Promise<CoreMonitor[]> {
  const id = `monitors-${Date.now()}-${++monitorRequestSequence}`
  let removeResponse: (() => void) | undefined
  let removeEvent: (() => void) | undefined
  let timer: ReturnType<typeof setTimeout> | undefined

  try {
    return await new Promise<CoreMonitor[]>((resolve, reject) => {
      let settled = false
      const cleanup = () => {
        if (timer !== undefined) clearTimeout(timer)
        removeResponse?.()
        removeEvent?.()
      }
      const fail = (message: string) => {
        if (settled) return
        settled = true
        cleanup()
        reject(new Error(message))
      }
      const complete = (monitors: CoreMonitor[]) => {
        if (settled) return
        settled = true
        cleanup()
        resolve(monitors)
      }

      void (async () => {
        try {
          removeResponse = await onCoreToolsResponse((response) => {
            if (response.id !== id) return
            if (!response.ok) {
              fail('モニター一覧の取得に失敗しました')
              return
            }
            const encoded = response.monitors_json
            if (typeof encoded !== 'string') {
              fail('モニター一覧の応答が不正です')
              return
            }
            try {
              const parsed: unknown = JSON.parse(encoded)
              if (!Array.isArray(parsed) || !parsed.every(isCoreMonitor)) {
                fail('モニター一覧の形式が不正です')
                return
              }
              complete(parsed.filter(isCoreMonitor))
            } catch {
              fail('モニター一覧のJSONを読み込めません')
            }
          })
          removeEvent = await onCoreToolsEvent((event) => {
            if (event.event === 'core_tools_terminated') {
              fail('処理エンジンが終了しました')
            }
          })
          if (settled) {
            cleanup()
            return
          }
          timer = setTimeout(
            () => fail('モニター一覧の取得がタイムアウトしました'),
            10_000,
          )
          await sendCoreToolsCommand(
            JSON.stringify({ id, cmd: 'list_monitors' }),
          )
        } catch {
          fail('処理エンジンへコマンドを送信できませんでした')
        }
      })()
    })
  } finally {
    if (timer !== undefined) clearTimeout(timer)
    removeResponse?.()
    removeEvent?.()
  }
}

/** MJPEG プレビューの URL。CSP が null なので <img src> で直接参照可能。 */
export const MJPEG_BASE_URL = 'http://127.0.0.1:39010'

/** 指定ロールの MJPEG ストリーム URL を返す。 */
export function mjpegStreamUrl(role: 'left' | 'right'): string {
  return `${MJPEG_BASE_URL}/${role}.mjpg`
}
