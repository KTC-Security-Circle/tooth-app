import { useSyncExternalStore } from 'react'

export type Theme = 'light' | 'dark' | 'system'

const STORAGE_KEY = 'theme'

function readTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'light' || stored === 'dark' || stored === 'system') {
      return stored
    }
  } catch {
    // localStorage unavailable — default to system
  }
  return 'system'
}

function getSystemResolved(): 'light' | 'dark' {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light'
}

function applyTheme(theme: Theme) {
  const resolved = theme === 'system' ? getSystemResolved() : theme
  document.documentElement.classList.toggle('dark', resolved === 'dark')
}

let currentTheme: Theme = readTheme()
const listeners = new Set<() => void>()

applyTheme(currentTheme)

window
  .matchMedia('(prefers-color-scheme: dark)')
  .addEventListener('change', () => {
    if (currentTheme === 'system') {
      applyTheme('system')
    }
  })

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): Theme {
  return currentTheme
}

function setTheme(next: Theme) {
  if (next === currentTheme) return
  currentTheme = next
  try {
    localStorage.setItem(STORAGE_KEY, next)
  } catch {
    // localStorage unavailable — keep in-memory only
  }
  applyTheme(next)
  for (const listener of listeners) {
    listener()
  }
}

export function useTheme() {
  const theme = useSyncExternalStore(subscribe, getSnapshot)
  return { theme, setTheme }
}
