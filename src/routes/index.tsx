import { createFileRoute } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { useSyncExternalStore } from 'react'
import PageHeader from '@/components/app/PageHeader'
import SectionPanel from '@/components/app/SectionPanel'
import StreamPreview from '@/components/app/StreamPreview'
import Button from '@/components/ui/Button'
import StatusBanner from '@/components/ui/StatusBanner'
import { useCoreToolsStream } from '@/hooks/useCoreToolsStream'
import type { Settings } from '@/hooks/useSettings'
import { cn } from '@/lib/cn'
import {
  getMatchingStatus,
  runMatching,
  subscribeMatching,
} from '@/lib/matching'

export const Route = createFileRoute('/')({
  loader: () =>
    invoke<Pick<Settings, 'cameraLeft' | 'cameraRight'>>('load_settings'),
  component: Index,
})

function Index() {
  const { cameraLeft, cameraRight } = Route.useLoaderData()
  const { status, streamState, streamActive, startStreams, stopStreams } =
    useCoreToolsStream()
  const matchingStatus = useSyncExternalStore(
    subscribeMatching,
    getMatchingStatus,
  )

  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="Tooth Calibrator"
        subtitle="歯科用キャリブレーションアプリケーションへようこそ。"
      />

      <StatusBanner status={status} />

      <SectionPanel heading="ステレオカメラプレビュー">
        <StreamPreview
          cameraLeft={cameraLeft}
          cameraRight={cameraRight}
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
          beforeControls={
            <div className="flex flex-col items-center gap-2">
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
                  'break-words text-center text-muted-foreground text-sm',
                  matchingStatus.type === 'error' && 'text-destructive',
                  matchingStatus.type === 'success' && 'text-success',
                )}
              >
                {matchingStatus.type !== 'idle' && matchingStatus.message}
              </p>
            </div>
          }
        />
      </SectionPanel>
    </div>
  )
}
