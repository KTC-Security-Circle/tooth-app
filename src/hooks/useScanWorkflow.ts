import { useCallback, useEffect, useRef, useState } from 'react'
import type { Status } from '@/components/ui/StatusBanner'
import { extractErrorMessage } from '@/lib/extractError'
import {
  getScanSession,
  moveTurntableSlot,
  preflightScan,
  resumeScanSession,
  retryScanSession,
  type ScanSessionManifest,
  startScanSession,
  zeroTurntable,
} from '@/lib/scan'

export const SESSION_SLOT_COUNT = 12

function generateSessionId(): string {
  const now = new Date()
  const yyyy = now.getFullYear()
  const mm = String(now.getMonth() + 1).padStart(2, '0')
  const dd = String(now.getDate()).padStart(2, '0')
  const hh = String(now.getHours()).padStart(2, '0')
  const mi = String(now.getMinutes()).padStart(2, '0')
  const ss = String(now.getSeconds()).padStart(2, '0')
  return `scan-${yyyy}${mm}${dd}-${hh}${mi}${ss}`
}

function buildInputDirs(baseDir: string): string[] {
  const trimmed = baseDir.trim()
  if (trimmed.length === 0) {
    return []
  }
  return Array.from(
    { length: SESSION_SLOT_COUNT },
    (_, slot) => `${trimmed}/${slot}`,
  )
}

export function useScanWorkflow() {
  const [status, setStatus] = useState<Status>({ type: 'idle' })
  const [preflight, setPreflight] = useState<{
    ready: boolean
    checks: Array<{ name: string; passed: boolean; message: string }>
  } | null>(null)
  const [isZeroed, setIsZeroed] = useState(false)
  const [currentSlot, setCurrentSlot] = useState<number | null>(null)
  const [session, setSession] = useState<ScanSessionManifest | null>(null)
  const [sessionId, setSessionId] = useState(generateSessionId)
  const [baseDir, setBaseDir] = useState('')
  const runningRef = useRef(false)

  const withStatus = useCallback(
    async <T>(
      operation: () => Promise<T>,
      loadingMessage: string,
      onSuccess?: (result: T) => string,
    ): Promise<T | undefined> => {
      setStatus({ type: 'loading', message: loadingMessage })
      try {
        const result = await operation()
        const message = onSuccess?.(result) ?? '操作が完了しました'
        setStatus({ type: 'success', message })
        return result
      } catch (e) {
        setStatus({ type: 'error', message: extractErrorMessage(e) })
        return undefined
      }
    },
    [],
  )

  const runPreflight = useCallback(async () => {
    const result = await withStatus(
      () => preflightScan(),
      'プリフライトチェックを実行中...',
      (r) =>
        r.ready
          ? 'スキャン設定の確認が完了しました'
          : 'スキャン設定に不備があります',
    )
    if (result !== undefined) {
      setPreflight(result)
    }
  }, [withStatus])

  const zero = useCallback(async () => {
    const result = await withStatus(
      () => zeroTurntable(),
      'ターンテーブルをゼロ位置に戻しています...',
      () => 'ターンテーブルをゼロ位置に設定しました',
    )
    if (result !== undefined) {
      setIsZeroed(true)
      setCurrentSlot(0)
    }
  }, [withStatus])

  const moveToSlot = useCallback(
    async (slot: number) => {
      if (slot < 0 || slot >= SESSION_SLOT_COUNT) {
        setStatus({
          type: 'error',
          message: 'スロット番号は 0 から 11 の間である必要があります',
        })
        return
      }
      const result = await withStatus(
        () => moveTurntableSlot(slot),
        `スロット ${slot} に移動中...`,
        () => `スロット ${slot} に移動しました`,
      )
      if (result !== undefined) {
        setCurrentSlot(slot)
      }
    },
    [withStatus],
  )

  const refreshSession = useCallback(async () => {
    const id = sessionId.trim()
    if (id.length === 0) {
      return
    }
    try {
      const result = await getScanSession({ session_id: id })
      setSession(result.session)
    } catch {
      setSession(null)
    }
  }, [sessionId])

  const startSession = useCallback(async () => {
    const id = sessionId.trim()
    if (id.length === 0) {
      setStatus({ type: 'error', message: 'セッション ID を入力してください' })
      return
    }
    const inputDirs = buildInputDirs(baseDir)
    if (inputDirs.length === 0) {
      setStatus({
        type: 'error',
        message: 'スキャン入力ディレクトリのベースパスを入力してください',
      })
      return
    }
    runningRef.current = true
    setStatus({
      type: 'loading',
      message: 'スキャンセッションを開始しています...',
    })
    try {
      const result = await startScanSession({
        session_id: id,
        input_dirs: inputDirs,
      })
      setSession(result.session)
      setStatus({
        type: 'success',
        message: 'スキャンセッションが完了しました',
      })
    } catch (e) {
      setStatus({ type: 'error', message: extractErrorMessage(e) })
      await refreshSession()
    } finally {
      runningRef.current = false
    }
  }, [baseDir, refreshSession, sessionId])

  const resumeSession = useCallback(async () => {
    const id = sessionId.trim()
    if (id.length === 0) {
      setStatus({ type: 'error', message: 'セッション ID を入力してください' })
      return
    }
    runningRef.current = true
    setStatus({
      type: 'loading',
      message: 'スキャンセッションを再開しています...',
    })
    try {
      const result = await resumeScanSession({ session_id: id })
      setSession(result.session)
      setStatus({
        type: 'success',
        message: 'スキャンセッションが完了しました',
      })
    } catch (e) {
      setStatus({ type: 'error', message: extractErrorMessage(e) })
      await refreshSession()
    } finally {
      runningRef.current = false
    }
  }, [refreshSession, sessionId])

  const retrySlot = useCallback(
    async (slot: number) => {
      const id = sessionId.trim()
      if (id.length === 0) {
        setStatus({
          type: 'error',
          message: 'セッション ID を入力してください',
        })
        return
      }
      if (slot < 0 || slot >= SESSION_SLOT_COUNT) {
        setStatus({
          type: 'error',
          message: 'スロット番号は 0 から 11 の間である必要があります',
        })
        return
      }
      runningRef.current = true
      setStatus({
        type: 'loading',
        message: `スロット ${slot} の再スキャンを開始しています...`,
      })
      try {
        const result = await retryScanSession({ session_id: id, slot })
        setSession(result.session)
        setStatus({
          type: 'success',
          message: `スロット ${slot} の再スキャンが完了しました`,
        })
      } catch (e) {
        setStatus({ type: 'error', message: extractErrorMessage(e) })
        await refreshSession()
      } finally {
        runningRef.current = false
      }
    },
    [refreshSession, sessionId],
  )

  useEffect(() => {
    const isRunning = session?.status === 'running' || runningRef.current
    if (!isRunning) {
      return
    }
    const timer = setInterval(() => {
      void refreshSession()
    }, 2000)
    return () => {
      clearInterval(timer)
    }
  }, [refreshSession, session?.status])

  return {
    status,
    preflight,
    isZeroed,
    currentSlot,
    session,
    sessionId,
    baseDir,
    setSessionId,
    setBaseDir,
    runPreflight,
    zero,
    moveToSlot,
    startSession,
    resumeSession,
    retrySlot,
    refreshSession,
  }
}

export function slotStatusLabel(
  status: ScanSessionManifest['slots'][number]['status'],
): string {
  switch (status) {
    case 'complete':
      return '完了'
    case 'needs_rescan':
      return '再スキャンが必要'
    case 'processing':
      return '処理中'
    default:
      return '未処理'
  }
}

export function sessionStatusLabel(
  status: ScanSessionManifest['status'],
): string {
  switch (status) {
    case 'running':
      return '実行中'
    case 'failed':
      return '失敗'
    case 'complete':
      return '完了'
    default:
      return '不明'
  }
}
