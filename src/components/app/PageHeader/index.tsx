interface Props {
  title: string
  subtitle?: string
  onTitleClick?: () => void
}

const PageHeader: React.FC<Props> = ({ title, subtitle, onTitleClick }) => {
  return (
    <header className="mb-6 border-slate-200 border-b pb-4">
      <h1
        className="inline-block font-semibold text-2xl text-slate-900"
        onClick={onTitleClick}
        onKeyDown={
          onTitleClick
            ? (e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  onTitleClick()
                }
              }
            : undefined
        }
      >
        {title}
      </h1>
      {subtitle !== undefined && (
        <p className="mt-1 text-slate-500 text-sm">{subtitle}</p>
      )}
    </header>
  )
}

export default PageHeader
