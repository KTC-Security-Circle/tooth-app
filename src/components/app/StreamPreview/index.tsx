import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import Button from '@/components/ui/Button'
import type { StreamState } from '@/hooks/useCoreToolsStream'
import type { Settings } from '@/hooks/useSettings'
import { resolveCameraSelection } from '@/lib/cameraSelection'
import { cn } from '@/lib/cn'
import { mjpegStreamUrl } from '@/lib/coreTools'
import { extractErrorMessage } from '@/lib/extractError'
import StreamPane from './StreamPane'

interface Props {
  streamState: StreamState
  streamActive: { left: boolean; right: boolean }
  startStreams: (leftCameraId: number, rightCameraId: number) => Promise<void>
  stopStreams: () => Promise<void>
  isLoading: boolean
}

const StreamPreview: React.FC<Props> & {
  Pane: typeof StreamPane
} = ({ streamState, streamActive, startStreams, stopStreams, isLoading }) => {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [settingsError, setSettingsError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    invoke<Settings>('load_settings')
      .then((loaded) => {
        if (!cancelled) setSettings(loaded)
      })
      .catch((error) => {
        if (!cancelled) {
          setSettingsError(
            `カメラ設定を読み込めませんでした: ${extractErrorMessage(error)}`,
          )
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  const selection = settings
    ? resolveCameraSelection(settings.cameraLeft, settings.cameraRight)
    : null
  const selectionError =
    settingsError ??
    (selection && 'error' in selection ? selection.error : null)
  const isIdle = streamState === 'idle'
  const isStreaming = streamState === 'streaming'
  const isStarting = streamState === 'starting'
  const isStopping = streamState === 'stopping'

  const canStart = isIdle && selection !== null && !selectionError
  const canStop = isStreaming

  const handleStart = async () => {
    if (!selection || 'error' in selection) return
    await startStreams(selection.leftCameraId, selection.rightCameraId)
  }

  const handleStop = async () => {
    await stopStreams()
  }

  return (
    <div className={cn('space-y-6')}>
      <div className={cn('grid grid-cols-1 gap-4', 'md:grid-cols-2')}>
        <StreamPane
          streamRole="left"
          label="Left"
          streamUrl={mjpegStreamUrl('left')}
          active={streamActive.left}
          loading={isStarting || isStopping}
        />
        <StreamPane
          streamRole="right"
          label="Right"
          streamUrl={mjpegStreamUrl('right')}
          active={streamActive.right}
          loading={isStarting || isStopping}
        />
      </div>

      {selectionError ? (
        <p className="text-center text-destructive text-sm" role="alert">
          {selectionError}
        </p>
      ) : !settings ? (
        <p className="text-center text-muted-foreground text-sm" role="status">
          カメラ設定を読み込み中...
        </p>
      ) : null}

      <div className="flex items-center justify-center gap-3">
        <Button disabled={!canStart || isLoading} onClick={handleStart}>
          {isStarting ? (
            <span className="flex items-center gap-2">
              <SpinnerIcon />
              開始中...
            </span>
          ) : (
            'ストリーム開始'
          )}
        </Button>
        <Button
          variant="secondary"
          disabled={!canStop || isLoading}
          onClick={handleStop}
        >
          {isStopping ? (
            <span className="flex items-center gap-2">
              <SpinnerIcon />
              停止中...
            </span>
          ) : (
            'ストリーム停止'
          )}
        </Button>
      </div>
    </div>
  )
}

const SpinnerIcon: React.FC = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="animate-spin"
    aria-hidden="true"
  >
    <path d="M21 12a9 9 0 1 1-6.219-8.56" />
  </svg>
)

StreamPreview.Pane = StreamPane

export default StreamPreview
