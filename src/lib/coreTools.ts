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

/** core-tools を起動する。ready event 受信 (10s タイムアウト) まで待つ。 */
export function startCoreTools(): Promise<void> {
  return invoke<void>('start_core_tools')
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

/** MJPEG プレビューの URL。CSP が null なので <img src> で直接参照可能。 */
export const MJPEG_BASE_URL = 'http://127.0.0.1:39010'

/** 指定ロールの MJPEG ストリーム URL を返す。 */
export function mjpegStreamUrl(role: 'left' | 'right'): string {
  return `${MJPEG_BASE_URL}/${role}.mjpg`
}
