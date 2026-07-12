import { cn } from '@/lib/cn'

interface Props extends React.ComponentPropsWithoutRef<'input'> {}

const TextInput: React.FC<Props> = ({ className, ...props }) => {
  return (
    <input
      className={cn(
        'flex-1 rounded-sm border border-slate-300 bg-white px-3 py-2 text-slate-900 text-sm outline-none focus:border-slate-500 focus:ring-1 focus:ring-slate-500',
        className,
      )}
      {...props}
    />
  )
}

export default TextInput
