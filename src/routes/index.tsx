import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/')({
  component: Index,
})

function Index() {
  return (
    <div className="mx-auto max-w-4xl p-6">
      <header className="mb-6 border-border border-b pb-4">
        <h1 className="font-semibold text-2xl text-foreground">
          Tooth Calibrator
        </h1>
        <p className="mt-1 text-muted-foreground text-sm">
          歯科用キャリブレーションアプリケーションへようこそ。
        </p>
      </header>

      <div className="rounded-lg border border-border bg-card p-5 shadow-sm">
        <p className="text-muted-foreground text-sm">
          設定が完了しました。右上の歯車アイコンから設定を開けます。
        </p>
      </div>
    </div>
  )
}
