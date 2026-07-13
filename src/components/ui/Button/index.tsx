import { cn } from '@/lib/cn'

interface Props extends React.ComponentPropsWithoutRef<'button'> {
  variant?: 'primary' | 'secondary'
}

const variantClasses: Record<NonNullable<Props['variant']>, string> = {
  primary:
    'rounded-lg bg-primary px-5 py-2 text-sm font-medium text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50',
  secondary:
    'rounded-lg border border-border bg-secondary px-4 py-2 text-sm font-medium text-secondary-foreground shadow-sm transition-colors hover:bg-accent focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50',
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
