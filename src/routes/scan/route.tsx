import { createFileRoute, Outlet } from '@tanstack/react-router'
import PageHeader from '@/components/app/PageHeader'

export const Route = createFileRoute('/scan')({
  component: ScanLayout,
})

function ScanLayout() {
  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="スキャンワークフロー"
        subtitle="設定済みのスキャン設定を使ってスキャンセッションを実行します。"
      />
      <Outlet />
    </div>
  )
}
