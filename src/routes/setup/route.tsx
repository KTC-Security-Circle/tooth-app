import { createFileRoute, Outlet } from '@tanstack/react-router'
import PageHeader from '@/components/app/PageHeader'

export const Route = createFileRoute('/setup')({
  component: RouteComponent,
})

function RouteComponent() {
  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="初期設定"
        subtitle="カメラの初回セットアップを行います。"
      />
      <Outlet />
    </div>
  )
}
