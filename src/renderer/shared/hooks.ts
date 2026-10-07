import { useEffect, useRef, useState } from 'react'
import type { Settings, Snapshot } from '../../shared/types'

export function useTheme(): 'light' | 'dark' {
  const [theme, setTheme] = useState<'light' | 'dark'>(() => (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'))
  const first = useRef(true)
  useEffect(() => {
    void window.localhost.theme().then(setTheme)
    return window.localhost.onTheme(setTheme)
  }, [])
  useEffect(() => {
    const root = document.documentElement
    if (!first.current) {
      root.classList.add('theme-transition')
      const t = setTimeout(() => root.classList.remove('theme-transition'), 520)
      root.dataset.theme = theme
      return () => clearTimeout(t)
    }
    first.current = false
    root.dataset.theme = theme
  }, [theme])
  return theme
}

export function useSettings(): Settings | null {
  const [s, setS] = useState<Settings | null>(null)
  useEffect(() => {
    void window.localhost.settings().then(setS)
    return window.localhost.onSettings(setS)
  }, [])
  useEffect(() => {
    if (s) document.documentElement.classList.toggle('reduced-motion', s.motion === 'reduced')
  }, [s])
  return s
}

export function useSnapshot(): Snapshot | null {
  const [s, setS] = useState<Snapshot | null>(null)
  useEffect(() => {
    void window.localhost.snapshot().then((v) => v && setS(v))
    return window.localhost.onSnapshot(setS)
  }, [])
  return s
}

export function useNow(ms = 1000): number {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms)
    return () => clearInterval(t)
  }, [ms])
  return now
}

export function platformClass(): void {
  document.documentElement.classList.add(`platform-${window.localhost.platform}`)
}
