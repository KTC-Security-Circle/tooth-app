import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useEffect, useState } from 'react'
import { extractErrorMessage } from '@/lib/extractError'

export type MatchingServerStatus =
  | 'idle'
  | 'starting'
  | 'ready'
  | 'disconnected'
  | 'failed'

type MatchingServerSnapshot = {
  status: MatchingServerStatus | null
  version: number
}

const initialSnapshot: MatchingServerSnapshot = { status: null, version: -1 }

export function selectMatchingServerSnapshot(
  current: MatchingServerSnapshot,
  value: unknown,
): MatchingServerSnapshot {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('version' in value) ||
    typeof value.version !== 'number' ||
    !Number.isSafeInteger(value.version) ||
    value.version < 0
  ) {
    return { ...current, status: null }
  }
  if (value.version <= current.version) return current
  const status = 'status' in value ? value.status : null
  return {
    version: value.version,
    status:
      status === 'idle' ||
      status === 'starting' ||
      status === 'ready' ||
      status === 'disconnected' ||
      status === 'failed'
        ? status
        : null,
  }
}

export function useMatchingServerStatus() {
  const [snapshot, setSnapshot] = useState(initialSnapshot)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    let unlisten: UnlistenFn | undefined
    let current = initialSnapshot
    const update = (value: unknown) => {
      if (cancelled) return
      const next = selectMatchingServerSnapshot(current, value)
      if (next === current) return
      current = next
      setSnapshot(next)
      setError(null)
    }

    void (async () => {
      try {
        unlisten = await listen<unknown>('matching-server:status', (event) => {
          update(event.payload)
        })
        if (cancelled) {
          unlisten()
          return
        }
        update(await invoke<unknown>('matching_server_status'))
      } catch (cause) {
        if (cancelled) return
        current = { ...current, status: null }
        setSnapshot(current)
        setError(
          `マッチング処理の状態を確認できませんでした: ${extractErrorMessage(cause)}`,
        )
      }
    })()

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  return { status: snapshot.status, error }
}
