import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useState } from 'react'
import SectionPanel from '@/components/app/SectionPanel'
import Button from '@/components/ui/Button'
import FormField from '@/components/ui/FormField'
import { extractErrorMessage } from '@/lib/extractError'

export type ScanMonitor = {
  index: number
  name: string
  width: number
  height: number
  primary: boolean
}

interface Props {
  monitorIndex?: number | null
  onMonitorIndexChange: (value: number | null) => void
}

const ScanMonitorSelectSection: React.FC<Props> = ({
  monitorIndex,
  onMonitorIndexChange,
}) => {
  const [monitors, setMonitors] = useState<ScanMonitor[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadMonitors = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setMonitors(await invoke<ScanMonitor[]>('list_scan_monitors'))
    } catch (e) {
      setError(`モニターの検出に失敗しました: ${extractErrorMessage(e)}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadMonitors()
  }, [loadMonitors])

  const selectedMonitor = monitors.find(
    (monitor) => monitor.index === monitorIndex,
  )

  return (
    <SectionPanel heading="スキャンモニター">
      <div className="space-y-4">
        <FormField>
          <FormField.Label htmlFor="scan-monitor">モニター</FormField.Label>
          <FormField.Select
            id="scan-monitor"
            value={monitorIndex == null ? '' : String(monitorIndex)}
            disabled={loading}
            onChange={(event) => {
              const value = event.currentTarget.value
              onMonitorIndexChange(value === '' ? null : Number(value))
            }}
          >
            <FormField.Select.Option value="">
              {loading ? '検出中...' : '未選択'}
            </FormField.Select.Option>
            {monitors.map((monitor) => (
              <FormField.Select.Option
                key={monitor.index}
                value={monitor.index}
              >
                {monitor.name} ({monitor.width}×{monitor.height}, 番号{' '}
                {monitor.index}){monitor.primary ? '・プライマリ' : ''}
              </FormField.Select.Option>
            ))}
          </FormField.Select>
        </FormField>

        {error ? (
          <p className="text-destructive text-sm" role="alert">
            {error}
          </p>
        ) : null}
        {!loading && !error && monitors.length === 0 ? (
          <p className="text-muted-foreground text-sm">
            利用できるモニターがありません。
          </p>
        ) : null}
        {!loading && selectedMonitor === undefined && monitorIndex != null ? (
          <p className="text-muted-foreground text-sm" role="status">
            保存済みのモニター（番号 {monitorIndex}）を一時的に検出できません。
          </p>
        ) : null}
        <Button
          variant="secondary"
          onClick={() => void loadMonitors()}
          disabled={loading}
        >
          {loading ? '検出中...' : '再検出'}
        </Button>
      </div>
    </SectionPanel>
  )
}

export default ScanMonitorSelectSection
