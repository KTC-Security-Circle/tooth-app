interface Props extends React.ComponentPropsWithoutRef<'option'> {}

const Option: React.FC<Props> = (props) => {
  return <option {...props} />
}

export default Option
