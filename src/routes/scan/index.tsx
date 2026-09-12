import { createFileRoute } from '@tanstack/react-router'
import SectionPanel from '@/components/app/SectionPanel'
import Button from '@/components/ui/Button'
import FormField from '@/components/ui/FormField'
import StatusBanner from '@/components/ui/StatusBanner'
import TextInput from '@/components/ui/TextInput'
import {
  SESSION_SLOT_COUNT,
  sessionStatusLabel,
  slotStatusLabel,
  useScanWorkflow,
} from '@/hooks/useScanWorkflow'

export const Route = createFileRoute('/scan/')({
  component: ScanPage,
})

const SLOTS = Array.from({ length: SESSION_SLOT_COUNT }, (_, index) => index)

function ScanPage() {
  const {
    status,
    preflight,
    isZeroed,
    currentSlot,
    session,
    sessionId,
    baseDir,
    setSessionId,
    setBaseDir,
    runPreflight,
    zero,
    moveToSlot,
    startSession,
    resumeSession,
    retrySlot,
    refreshSession,
  } = useScanWorkflow()

  const isRunning = status.type === 'loading'
  const canControlTurntable = !isRunning
  const canStartSession = preflight?.ready === true && isZeroed && !isRunning

  return (
    <div className="space-y-8">
      <StatusBanner status={status} />

      <SectionPanel heading="プリフライトチェック">
        <div className="space-y-4">
          <p className="text-muted-foreground text-sm">
            スキャン開始前に必要な設定とファイルの存在を確認します。
          </p>
          <div className="flex items-center gap-4">
            <Button disabled={isRunning} onClick={runPreflight}>
              プリフライトを実行
            </Button>
            {preflight !== null && (
              <span
                className={
                  preflight.ready
                    ? 'font-medium text-sm text-success'
                    : 'font-medium text-destructive text-sm'
                }
              >
                {preflight.ready ? '準備完了' : '準備未完了'}
              </span>
            )}
          </div>
          {preflight !== null && (
            <ul className="space-y-2">
              {preflight.checks.map((check) => (
                <li key={check.name} className="flex items-start gap-2 text-sm">
                  <span
                    className={
                      check.passed ? 'text-success' : 'text-destructive'
                    }
                  >
                    {check.passed ? '✓' : '✗'}
                  </span>
                  <span className="text-foreground">{check.message}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </SectionPanel>

      <SectionPanel heading="ターンテーブル制御">
        <div className="space-y-5">
          <div className="flex items-center gap-4">
            <Button
              variant="secondary"
              disabled={!canControlTurntable}
              onClick={zero}
            >
              ゼロ位置に戻す
            </Button>
            <span className="text-muted-foreground text-sm">
              {isZeroed ? 'ゼロ位置を設定済み' : 'ゼロ位置が未設定です'}
            </span>
          </div>

          <div>
            <p className="mb-3 text-muted-foreground text-sm">スロット移動</p>
            <div className="grid grid-cols-6 gap-2 sm:grid-cols-12">
              {SLOTS.map((slot) => (
                <Button
                  key={`move-slot-${slot}`}
                  variant={currentSlot === slot ? 'primary' : 'secondary'}
                  className="px-0"
                  disabled={!canControlTurntable || !isZeroed}
                  onClick={() => {
                    void moveToSlot(slot)
                  }}
                >
                  {slot}
                </Button>
              ))}
            </div>
          </div>
        </div>
      </SectionPanel>

      <SectionPanel heading="スキャンセッション">
        <div className="space-y-5">
          <FormField labelWidth="160px">
            <FormField.Label htmlFor="session-id">
              セッション ID
            </FormField.Label>
            <TextInput
              id="session-id"
              value={sessionId}
              onChange={(e) => {
                setSessionId(e.currentTarget.value)
              }}
            />
          </FormField>

          <FormField labelWidth="160px">
            <FormField.Label htmlFor="base-dir">
              入力ディレクトリのベースパス
            </FormField.Label>
            <TextInput
              id="base-dir"
              value={baseDir}
              placeholder="/path/to/scan"
              onChange={(e) => {
                setBaseDir(e.currentTarget.value)
              }}
            />
          </FormField>

          <div className="flex flex-wrap gap-3">
            <Button
              disabled={!canStartSession}
              onClick={() => {
                void startSession()
              }}
            >
              開始
            </Button>
            <Button
              variant="secondary"
              disabled={!isZeroed || isRunning}
              onClick={() => {
                void resumeSession()
              }}
            >
              再開
            </Button>
            <Button
              variant="secondary"
              disabled={isRunning}
              onClick={() => {
                void refreshSession()
              }}
            >
              状態を更新
            </Button>
          </div>
        </div>
      </SectionPanel>

      {session !== null && (
        <SectionPanel heading="セッション状態">
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-4 text-sm">
              <span className="text-muted-foreground">状態:</span>
              <span className="font-medium text-foreground">
                {sessionStatusLabel(session.status)}
              </span>
              {session.current_slot !== null && (
                <>
                  <span className="text-muted-foreground">現在のスロット:</span>
                  <span className="font-medium text-foreground">
                    {session.current_slot}
                  </span>
                </>
              )}
            </div>
            {session.reason !== null && (
              <p className="text-destructive text-sm">{session.reason}</p>
            )}

            <div>
              <p className="mb-3 text-muted-foreground text-sm">スロット一覧</p>
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
                {session.slots.map((slot) => (
                  <div
                    key={slot.slot}
                    className={`rounded-lg border p-3 text-sm ${
                      slot.status === 'complete'
                        ? 'border-success/30 bg-success/10'
                        : slot.status === 'needs_rescan'
                          ? 'border-destructive/30 bg-destructive/10'
                          : slot.status === 'processing'
                            ? 'border-primary/30 bg-primary/10'
                            : 'border-border bg-muted'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-foreground">
                        スロット {slot.slot}
                      </span>
                      {slot.status === 'needs_rescan' && (
                        <Button
                          variant="secondary"
                          className="px-2 py-1 text-xs"
                          disabled={isRunning}
                          onClick={() => {
                            void retrySlot(slot.slot)
                          }}
                        >
                          再試行
                        </Button>
                      )}
                    </div>
                    <p className="mt-1 text-muted-foreground text-xs">
                      {slotStatusLabel(slot.status)}
                    </p>
                    {slot.reason !== null && (
                      <p className="mt-1 text-destructive text-xs">
                        {slot.reason}
                      </p>
                    )}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </SectionPanel>
      )}
    </div>
  )
}
