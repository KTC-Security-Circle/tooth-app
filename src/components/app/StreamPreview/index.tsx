import Button from '@/components/ui/Button'
import type { StreamState } from '@/hooks/useCoreToolsStream'
import { cn } from '@/lib/cn'
import { mjpegStreamUrl } from '@/lib/coreTools'
import StreamPane from './StreamPane'

interface Props {
  cameraLeft: string
  cameraRight: string
  streamState: StreamState
  streamActive: { left: boolean; right: boolean }
  startStreams: (leftCameraId: number, rightCameraId: number) => Promise<void>
  stopStreams: () => Promise<void>
  isLoading: boolean
  beforeControls?: React.ReactNode
}

function cameraIndex(path: string): number | null {
  const match = /^\/dev\/video(\d+)$/.exec(path)
  if (!match) return null

  const index = Number(match[1])
  return Number.isSafeInteger(index) && index <= 2147483647 ? index : null
}

const StreamPreview: React.FC<Props> & {
  Pane: typeof StreamPane
} = ({
  cameraLeft,
  cameraRight,
  streamState,
  streamActive,
  startStreams,
  stopStreams,
  isLoading,
  beforeControls,
}) => {
  const leftCameraId = cameraIndex(cameraLeft)
  const rightCameraId = cameraIndex(cameraRight)

  const cameraSelectionError =
    leftCameraId === null || rightCameraId === null
      ? '設定画面で左右のカメラを選択してください。'
      : leftCameraId === rightCameraId
        ? '左右には異なるカメラを選択してください。'
        : null

  const isIdle = streamState === 'idle'
  const isStreaming = streamState === 'streaming'
  const isStarting = streamState === 'starting'
  const isStopping = streamState === 'stopping'

  const canStart = isIdle || isStopping
  const canStop = isStreaming

  const handleStart = async () => {
    if (
      cameraSelectionError !== null ||
      leftCameraId === null ||
      rightCameraId === null
    ) {
      return
    }

    await startStreams(leftCameraId, rightCameraId)
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

      {beforeControls}

      <div className="flex items-center justify-center gap-3">
        <Button
          disabled={!canStart || isLoading || cameraSelectionError !== null}
          onClick={handleStart}
        >
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
        {cameraSelectionError && (
          <p role="alert" className="text-center text-destructive text-sm">
            {cameraSelectionError}
          </p>
        )}
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
