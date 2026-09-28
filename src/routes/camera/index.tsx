import { createFileRoute } from '@tanstack/react-router'
import PageHeader from '@/components/app/PageHeader'
import SectionPanel from '@/components/app/SectionPanel'
import StreamPreview from '@/components/app/StreamPreview'
import StatusBanner from '@/components/ui/StatusBanner'
import { useCoreToolsStream } from '@/hooks/useCoreToolsStream'

export const Route = createFileRoute('/camera/')({
  component: Camera,
})

function Camera() {
  const { status, streamState, streamActive, startStreams, stopStreams } =
    useCoreToolsStream()

  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="カメラ"
        subtitle="ステレオカメラの映像を確認します。"
      />
      <StatusBanner status={status} />
      <SectionPanel heading="ステレオカメラプレビュー">
        <StreamPreview
          streamState={streamState}
          streamActive={streamActive}
          startStreams={startStreams}
          stopStreams={stopStreams}
          isLoading={status.type === 'loading'}
        />
      </SectionPanel>
    </div>
  )
}
