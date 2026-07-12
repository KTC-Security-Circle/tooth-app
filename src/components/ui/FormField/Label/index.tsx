import { cn } from '@/lib/cn'

interface Props {
  htmlFor: string
  className?: string
  children: React.ReactNode
}

const Label: React.FC<Props> = ({ htmlFor, className, children }) => {
  return (
    <label
      className={cn(
        'self-center font-medium text-slate-700 text-sm',
        className,
      )}
      htmlFor={htmlFor}
    >
      {children}
    </label>
  )
}

export default Label
