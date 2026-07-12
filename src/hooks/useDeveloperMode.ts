import { useRef } from 'react'

export function useDeveloperMode(
  isEnabled: boolean,
  onActivate: () => void | Promise<void>,
) {
  const devClicks = useRef(0)
  const devClickTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleLabelClick = async () => {
    if (isEnabled) {
      return
    }

    devClicks.current += 1

    if (devClickTimer.current !== null) {
      clearTimeout(devClickTimer.current)
    }

    devClickTimer.current = setTimeout(() => {
      devClicks.current = 0
    }, 2000)

    if (devClicks.current >= 7) {
      devClicks.current = 0
      if (devClickTimer.current !== null) {
        clearTimeout(devClickTimer.current)
        devClickTimer.current = null
      }

      await onActivate()
    }
  }

  return { handleLabelClick }
}
