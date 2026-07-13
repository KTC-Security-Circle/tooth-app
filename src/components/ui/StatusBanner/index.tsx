import { cn } from '@/lib/cn'

export type Status =
  | { type: 'idle' }
  | { type: 'loading'; message: string }
  | { type: 'success'; message: string }
  | { type: 'error'; message: string }

interface Props {
  status: Status
}

const StatusBanner: React.FC<Props> = ({ status }) => {
  if (status.type === 'idle') {
    return null
  }

  return (
    <div
      className={cn(
        'mb-6 rounded-lg border px-4 py-3 text-sm',
        status.type === 'error' &&
          'border-destructive/30 bg-destructive/10 text-destructive',
        status.type === 'success' &&
          'border-success/30 bg-success/10 text-success',
        status.type === 'loading' &&
          'border-border bg-muted text-muted-foreground',
      )}
    >
      {status.message}
    </div>
  )
}

export default StatusBanner
