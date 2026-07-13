import { cn } from '@/lib/cn'

interface Props {
  heading: string
  className?: string
  children: React.ReactNode
}

const SectionPanel: React.FC<Props> = ({ heading, className, children }) => {
  return (
    <section
      className={cn(
        'rounded-lg border border-border bg-card p-5 shadow-sm',
        className,
      )}
    >
      <h2 className="mb-4 font-semibold text-muted-foreground text-sm uppercase tracking-wide">
        {heading}
      </h2>
      {children}
    </section>
  )
}

export default SectionPanel
