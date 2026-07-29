import { useEffect, useState } from 'react'
import { cn } from '@/lib/cn'

interface Props {
  streamRole: 'left' | 'right'
  label: string
  streamUrl: string
  active: boolean
  loading: boolean
}

const StreamPane: React.FC<Props> = ({
  streamRole,
  label,
  streamUrl,
  active,
  loading,
}) => {
  const [hasError, setHasError] = useState(false)
  const showStream = active && !hasError

  // biome-ignore lint/correctness/useExhaustiveDependencies: streamUrl is intentionally included to reset errors on URL change
  useEffect(() => {
    if (active) {
      setHasError(false)
    }
  }, [active, streamUrl])

  return (
    <div
      data-role={streamRole}
      className={cn(
        'relative flex aspect-4/3 flex-col overflow-hidden rounded-lg border border-border bg-muted',
      )}
    >
      <div className="absolute top-0 left-0 z-10 rounded-br-md bg-background/80 px-2 py-1 font-medium text-muted-foreground text-xs backdrop-blur-sm">
        {label}
      </div>

      {showStream ? (
        <img
          src={streamUrl}
          alt={`${label} カメラストリーム`}
          className="h-full w-full object-cover"
          onError={() => setHasError(true)}
        />
      ) : (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-muted-foreground">
          <CameraPlaceholderIcon />
          <p className="font-medium text-sm">
            {hasError
              ? 'ストリームの読み込みに失敗しました'
              : 'ストリーム未開始'}
          </p>
          {hasError && (
            <p className="text-xs">ストリームを再起動してください</p>
          )}
        </div>
      )}

      {loading && (
        <div className="absolute inset-0 z-20 flex items-center justify-center bg-background/70 backdrop-blur-sm">
          <div className="flex items-center gap-2 font-medium text-muted-foreground text-sm">
            <Spinner />
            <span>読み込み中...</span>
          </div>
        </div>
      )}
    </div>
  )
}

const CameraPlaceholderIcon: React.FC = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="40"
    height="40"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-muted-foreground/60"
    aria-hidden="true"
  >
    <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
    <circle cx="12" cy="13" r="3" />
  </svg>
)

const Spinner: React.FC = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="animate-spin"
    aria-hidden="true"
  >
    <path d="M21 12a9 9 0 1 1-6.219-8.56" />
  </svg>
)

export default StreamPane
