import { createFileRoute, Link } from '@tanstack/react-router'
import { useState } from 'react'
import PageHeader from '@/components/app/PageHeader'
import SectionPanel from '@/components/app/SectionPanel'
import Button from '@/components/ui/Button'
import FormField from '@/components/ui/FormField'
import StatusBanner, { type Status } from '@/components/ui/StatusBanner'
import TextInput from '@/components/ui/TextInput'
import { extractErrorMessage } from '@/lib/extractError'
import { moveTurntable } from '@/lib/turntable'

export const Route = createFileRoute('/turntable')({
  component: TurntableTest,
})

function TurntableTest() {
  const [angle, setAngle] = useState('')
  const [status, setStatus] = useState<Status>({ type: 'idle' })

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()

    const value = Number.parseFloat(angle)
    if (!Number.isFinite(value)) {
      setStatus({ type: 'error', message: '有効な数値を入力してください' })
      return
    }

    setStatus({ type: 'loading', message: 'ターンテーブルを回転中...' })
    try {
      await moveTurntable(value)
      setStatus({
        type: 'success',
        message: `${value}° の位置へ回転しました`,
      })
    } catch (e) {
      setStatus({
        type: 'error',
        message: `回転に失敗しました: ${extractErrorMessage(e)}`,
      })
    }
  }

  return (
    <div className="mx-auto max-w-4xl p-6">
      <PageHeader
        title="ターンテーブル診断"
        subtitle="指定した角度までターンテーブルを回転させます。"
      />

      <Link
        to="/"
        className="mb-6 inline-flex items-center gap-1 text-muted-foreground text-sm transition-colors hover:text-foreground"
      >
        ← ホーム
      </Link>

      <StatusBanner status={status} />

      <SectionPanel heading="角度制御">
        <form onSubmit={handleSubmit} className="space-y-5">
          <FormField labelWidth="120px">
            <FormField.Label htmlFor="turntable-angle">角度</FormField.Label>
            <TextInput
              id="turntable-angle"
              type="number"
              step={0.1}
              value={angle}
              placeholder="0"
              disabled={status.type === 'loading'}
              onChange={(e) => {
                setAngle(e.currentTarget.value)
              }}
            />
          </FormField>

          <div className="flex justify-center">
            <Button type="submit" disabled={status.type === 'loading'}>
              回転
            </Button>
          </div>
        </form>
      </SectionPanel>
    </div>
  )
}
