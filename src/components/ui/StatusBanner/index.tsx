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
        'mb-6 rounded-sm border px-4 py-3 text-sm',
        status.type === 'error' && 'border-red-200 bg-red-50 text-red-700',
        status.type === 'success' &&
          'border-emerald-200 bg-emerald-50 text-emerald-700',
        status.type === 'loading' &&
          'border-slate-200 bg-slate-100 text-slate-600',
      )}
    >
      {status.message}
    </div>
  )
}

export default StatusBanner
