import { cn } from '@/lib/cn'
import Control from './Control'
import Label from './Label'
import Select from './Select'

type FormFieldStyle = React.CSSProperties & {
  '--form-field-label-width': string
}

interface Props {
  labelWidth?: string
  className?: string
  children: React.ReactNode
}

const FormField: React.FC<Props> & {
  Label: typeof Label
  Control: typeof Control
  Select: typeof Select
} = ({ labelWidth = '120px', className, children }) => {
  const style: FormFieldStyle = { '--form-field-label-width': labelWidth }

  return (
    <div
      className={cn(
        'grid gap-4 sm:grid-cols-[var(--form-field-label-width)_1fr]',
        className,
      )}
      style={style}
    >
      {children}
    </div>
  )
}

FormField.Label = Label
FormField.Control = Control
FormField.Select = Select

export default FormField
