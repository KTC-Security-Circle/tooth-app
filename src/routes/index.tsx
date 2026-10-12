import { createFileRoute } from '@tanstack/react-router'
import { useSyncExternalStore } from 'react'
import SectionPanel from '@/components/app/SectionPanel'
import StreamPreview from '@/components/app/StreamPreview'
import Button from '@/components/ui/Button'
import StatusBanner from '@/components/ui/StatusBanner'
import { useCoreToolsStream } from '@/hooks/useCoreToolsStream'
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
  const { status, streamState, streamActive, startStreams, stopStreams } =
    useCoreToolsStream()
  const matchingStatus = useSyncExternalStore(
    subscribeMatching,
    getMatchingStatus,
  )

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
            disabled={
              matchingStatus.type === 'loading' ||
              streamState === 'starting' ||
              streamState === 'stopping'
            }
            onClick={() => void runMatching()}
          >
            {matchingStatus.type === 'loading'
              ? 'スキャン・照合中...'
              : 'マッチング開始'}
          </Button>
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
