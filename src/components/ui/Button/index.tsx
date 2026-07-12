import { cn } from '@/lib/cn'

interface Props extends React.ComponentPropsWithoutRef<'button'> {
  variant?: 'primary' | 'secondary'
}

const variantClasses: Record<NonNullable<Props['variant']>, string> = {
  primary:
    'rounded-sm bg-slate-800 px-5 py-2 font-medium text-sm text-white shadow-sm transition-colors hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-slate-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50',
  secondary:
    'rounded-sm border border-slate-300 bg-white px-4 py-2 font-medium text-slate-700 text-sm shadow-sm transition-colors hover:bg-slate-50 focus:outline-none focus:ring-2 focus:ring-slate-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50',
}

const Button: React.FC<Props> = ({
  variant = 'primary',
  className,
  type = 'button',
  children,
  ...props
}) => {
  return (
    <button
      type={type}
      className={cn(variantClasses[variant], className)}
      {...props}
    >
      {children}
    </button>
  )
}

export default Button
