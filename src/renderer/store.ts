import { create } from 'zustand'

export type Page = { kind: 'all' } | { kind: 'ports' } | { kind: 'other' } | { kind: 'settings' } | { kind: 'project'; id: string }
export type Tab = 'overview' | 'logs' | 'environment' | 'processes'

export interface Dialog {
  title: string
  body: string
  confirm: string
  danger?: boolean
  onConfirm: () => void | Promise<void>
}

export interface Toast {
  id: number
  text: string
  action?: { label: string; run: () => void }
}

interface State {
  page: Page
  detail: string | null
  tab: Tab
  query: string
  dialog: Dialog | null
  toast: Toast | null
  go: (page: Page) => void
  open: (id: string, tab?: Tab) => void
  close: () => void
  setTab: (tab: Tab) => void
  setQuery: (q: string) => void
  confirm: (d: Dialog) => void
  dismissDialog: () => void
  flash: (text: string, action?: Toast['action']) => void
  dismissToast: () => void
}

let toastId = 0
let toastTimer: ReturnType<typeof setTimeout> | null = null

export const useStore = create<State>((set) => ({
  page: { kind: 'all' },
  detail: null,
  tab: 'overview',
  query: '',
  dialog: null,
  toast: null,
  go: (page) => set({ page, detail: null }),
  open: (id, tab) => set((s) => ({ detail: id, tab: tab ?? (s.detail === id ? s.tab : 'overview') })),
  close: () => set({ detail: null }),
  setTab: (tab) => set({ tab }),
  setQuery: (query) => set({ query }),
  confirm: (dialog) => set({ dialog }),
  dismissDialog: () => set({ dialog: null }),
  flash: (text, action) => {
    const id = ++toastId
    if (toastTimer) clearTimeout(toastTimer)
    toastTimer = setTimeout(() => set((s) => (s.toast?.id === id ? { toast: null } : s)), action ? 6000 : 2800)
    set({ toast: { id, text, action } })
  },
  dismissToast: () => set({ toast: null }),
}))

export const store = useStore.getState
