import { createFileRoute, Link, Outlet } from '@tanstack/react-router'
import PageHeader from '@/components/app/PageHeader'

export const Route = createFileRoute('/settings')({
  component: SettingsLayout,
})

function SettingsLayout() {
  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="設定"
        subtitle="カメラとキャリブレーションの設定を行います。"
      />

      <Link
        to="/"
        className="mb-6 inline-flex items-center gap-1 text-muted-foreground text-sm transition-colors hover:text-foreground"
      >
        ← ホーム
      </Link>

      <Outlet />
    </div>
  )
}
