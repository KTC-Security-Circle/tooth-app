/**
 * Tauri コマンド (invoke) が reject したときの構造化エラー。
 * Rust 側の `AppError` が `{ kind, message }` 形式でシリアライズされる。
 */
export type InvokeError = {
  kind: string
  message: string
}

function isInvokeError(e: unknown): e is InvokeError {
  return (
    typeof e === 'object' &&
    e !== null &&
    'kind' in e &&
    typeof e.kind === 'string' &&
    'message' in e &&
    typeof e.message === 'string'
  )
}

/**
 * invoke の catch で捕捉した未知のエラーから表示用メッセージを取り出す。
 * - 構造化エラー (`{ kind, message }`) の場合は `message` を返す
 * - 文字列の場合はそのまま返す
 * - それ以外は `String(e)` にフォールバックする
 */
export function extractErrorMessage(e: unknown): string {
  if (typeof e === 'string') return e
  if (isInvokeError(e)) return e.message
  return String(e)
}
