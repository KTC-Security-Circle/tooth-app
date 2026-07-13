import { cn } from '@/lib/cn'

interface Props extends React.ComponentPropsWithoutRef<'input'> {}

const TextInput: React.FC<Props> = ({ className, ...props }) => {
  return (
    <input
      className={cn(
        'flex-1 rounded-lg border border-border bg-card px-3 py-2 text-card-foreground text-sm outline-none focus:border-ring focus:ring-1 focus:ring-ring',
        className,
      )}
      {...props}
    />
  )
}

export default TextInput
