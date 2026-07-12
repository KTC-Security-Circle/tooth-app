import { cn } from '@/lib/cn'
import Option from './Option'

interface Props extends React.ComponentPropsWithoutRef<'select'> {}

const Select: React.FC<Props> & {
  Option: typeof Option
} = ({ className, children, ...props }) => {
  return (
    <select
      className={cn(
        'rounded-sm border border-slate-300 bg-white px-3 py-2 text-slate-900 text-sm outline-none focus:border-slate-500 focus:ring-1 focus:ring-slate-500',
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
