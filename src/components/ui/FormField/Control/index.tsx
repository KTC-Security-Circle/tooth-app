import { cn } from '@/lib/cn'

interface Props {
  className?: string
  children: React.ReactNode
}

const Control: React.FC<Props> = ({ className, children }) => {
  return <div className={cn(className)}>{children}</div>
}

export default Control
