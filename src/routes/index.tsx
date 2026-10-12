import { createFileRoute } from '@tanstack/react-router'
import { useSyncExternalStore } from 'react'
import SectionPanel from '@/components/app/SectionPanel'
import StreamPreview from '@/components/app/StreamPreview'
import Button from '@/components/ui/Button'
import StatusBanner from '@/components/ui/StatusBanner'
import { useCoreToolsStream } from '@/hooks/useCoreToolsStream'
import { useMatchingServerStatus } from '@/hooks/useMatchingServerStatus'
import { cn } from '@/lib/cn'
import {
  getMatchingStatus,
  runMatching,
  subscribeMatching,
} from '@/lib/matching'

export const Route = createFileRoute('/')({
  component: Index,
})

function Index() {
  const {
    status,
    coreStatus,
    streamState,
    streamActive,
    startStreams,
    stopStreams,
  } = useCoreToolsStream()
  const matchingServer = useMatchingServerStatus()
  const matchingStatus = useSyncExternalStore(
    subscribeMatching,
    getMatchingStatus,
  )
  const enginesReady =
    coreStatus === 'running' && matchingServer.status === 'ready'
  const canMatch =
    enginesReady &&
    matchingStatus.type !== 'loading' &&
    streamState !== 'starting' &&
    streamState !== 'stopping'

  return (
    <div data-home className="flex flex-1 flex-col">
      <div className="m-auto w-full max-w-4xl p-6">
        <StatusBanner status={status} />

        <SectionPanel heading="ステレオカメラプレビュー">
          <StreamPreview
            streamState={streamState}
            streamActive={streamActive}
            startStreams={async (leftCameraId, rightCameraId) => {
              if (getMatchingStatus().type !== 'loading') {
                await startStreams(leftCameraId, rightCameraId)
              }
            }}
            stopStreams={async () => {
              if (getMatchingStatus().type !== 'loading') {
                await stopStreams()
              }
            }}
            isLoading={
              status.type === 'loading' || matchingStatus.type === 'loading'
            }
          />
        </SectionPanel>

        <div className="mt-6 flex flex-col items-center gap-2">
          <Button
            disabled={!canMatch}
            aria-describedby={
              !enginesReady ? 'matching-ready-status' : undefined
            }
            onClick={() => {
              if (canMatch && getMatchingStatus().type !== 'loading') {
                void runMatching()
              }
            }}
          >
            {matchingStatus.type === 'loading'
              ? 'スキャン・照合中...'
              : 'マッチング開始'}
          </Button>
          {!enginesReady && (
            <p
              id="matching-ready-status"
              role="status"
              className="wrap-anywhere text-center text-muted-foreground text-xs"
            >
              {matchingServer.error ??
                (matchingServer.status === 'failed'
                  ? 'マッチング処理の起動に失敗しました。'
                  : matchingServer.status === 'disconnected'
                    ? 'マッチング処理が切断されました。'
                    : '処理エンジンとマッチング処理の準備を待っています。')}
            </p>
          )}
          <p
            role="status"
            aria-live="polite"
            className={cn(
              'wrap-anywhere text-center text-muted-foreground text-sm',
              matchingStatus.type === 'error' && 'text-destructive',
              matchingStatus.type === 'success' && 'text-success',
            )}
          >
            {matchingStatus.type !== 'idle' && matchingStatus.message}
          </p>
        </div>
      </div>
    </div>
  )
}
