import { createFileRoute, redirect } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'

export const Route = createFileRoute('/')({
  beforeLoad: async () => {
    const exists = await invoke<boolean>('settings_exists')
    if (!exists) {
      throw redirect({ to: '/setup' })
    }
  },
  component: Index,
})

function Index() {
  return (
    <div className="mx-auto max-w-4xl p-6">
      <header className="mb-6 border-slate-200 border-b pb-4">
        <h1 className="font-semibold text-2xl text-slate-900">
          Tooth Calibrator
        </h1>
        <p className="mt-1 text-slate-500 text-sm">
          歯科用キャリブレーションアプリケーションへようこそ。
        </p>
      </header>

      <div className="rounded-sm border border-slate-200 bg-white p-5 shadow-sm">
        <p className="text-slate-600 text-sm">
          設定が完了しました。左上のナビゲーションから設定やアプリケーションの利用を開始できます。
        </p>
      </div>
    </div>
  )
}
