import Button from '@/components/ui/Button'
import type { StreamState } from '@/hooks/useCoreToolsStream'
import { cn } from '@/lib/cn'
import { mjpegStreamUrl } from '@/lib/coreTools'
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
  const isIdle = streamState === 'idle'
  const isStreaming = streamState === 'streaming'
  const isStarting = streamState === 'starting'
  const isStopping = streamState === 'stopping'

  const canStart = isIdle || isStopping
  const canStop = isStreaming

  const handleStart = async () => {
    // TODO: Settings にはカメラ path 文字列しかなく、core-tools の open_camera は
    // camera_id (数値) を要求する。path → camera_id の変換は今回スコープ外。
    // serve_app_flow_test.sh の LEFT_CAMERA=0, RIGHT_CAMERA=2 に倣い、仮に固定値を渡す。
    await startStreams(0, 2)
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
