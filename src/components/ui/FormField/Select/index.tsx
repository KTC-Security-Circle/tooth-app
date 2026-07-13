import { cn } from '@/lib/cn'
import Option from './Option'

interface Props extends React.ComponentPropsWithoutRef<'select'> {}

const Select: React.FC<Props> & {
  Option: typeof Option
} = ({ className, children, ...props }) => {
  return (
    <select
      className={cn(
        'rounded-lg border border-border bg-card px-3 py-2 text-card-foreground text-sm outline-none focus:border-ring focus:ring-1 focus:ring-ring',
        className,
      )}
      {...props}
    >
      {children}
    </select>
  )
}

Select.Option = Option

export default Select
