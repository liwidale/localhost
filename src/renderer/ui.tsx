import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import iconUrl from './assets/icon.png'
import { tr } from './shared/i18n'
import { Icon, type IconName } from './shared/icons'
import { useStore } from './store'

export function Toggle({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button role="switch" aria-checked={on} aria-label={label} className={`toggle${on ? ' on' : ''}`} onClick={() => onChange(!on)}>
      <motion.span className="knob" layout transition={{ type: 'spring', stiffness: 700, damping: 40 }} />
    </button>
  )
}

export function Row({ title, desc, children }: { title: string; desc?: ReactNode; children?: ReactNode }) {
  return (
    <div className="row">
      <div className="row-text">
        <div className="row-title">{title}</div>
        {desc && <div className="row-desc">{desc}</div>}
      </div>
      {children && <div className="row-control">{children}</div>}
    </div>
  )
}

export function Field({ title, children, mono }: { title: string; children: ReactNode; mono?: boolean }) {
  return (
    <div className="row kv">
      <div className="row-title">{title}</div>
      <div className={`row-value selectable${mono ? ' mono' : ''}`}>{children}</div>
    </div>
  )
}

export function Group({ title, children, foot, head }: { title?: string; children: ReactNode; foot?: ReactNode; head?: ReactNode }) {
  return (
    <section className="group">
      {(title || head) && (
        <div className="group-head">
          {title && <h3 className="group-title">{title}</h3>}
          {head}
        </div>
      )}
      <div className="group-box">{children}</div>
      {foot && <div className="group-foot">{foot}</div>}
    </section>
  )
}

export function Select<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string }[]
  onChange: (v: T) => void
  label: string
}) {
  return (
    <div className="select-wrap">
      <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value as T)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <Icon name="down" size={12} />
    </div>
  )
}

export function Seg<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string }[]
  onChange: (v: T) => void
  label: string
}) {
  const id = options.map((x) => x.value).join()
  return (
    <div className="seg" role="tablist" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} role="tab" aria-selected={value === o.value} className={value === o.value ? 'on' : ''} onClick={() => onChange(o.value)}>
          {value === o.value && <motion.span layoutId={`seg-${id}`} className="seg-bg" transition={{ type: 'spring', stiffness: 500, damping: 38 }} />}
          <span>{o.label}</span>
        </button>
      ))}
    </div>
  )
}

type BtnKind = 'primary' | 'secondary' | 'danger' | 'ghost'

export function Btn({
  children,
  kind = 'secondary',
  onClick,
  disabled,
  icon,
  small,
  title,
  on,
}: {
  children?: ReactNode
  kind?: BtnKind
  onClick?: () => void
  disabled?: boolean
  icon?: IconName
  small?: boolean
  title?: string
  on?: boolean
}) {
  const cls = ['sbtn', `sbtn-${kind}`, small && 'small', !children && 'icon', on && 'on'].filter(Boolean).join(' ')
  return (
    <button className={cls} onClick={onClick} disabled={disabled} title={title} aria-label={children ? undefined : title} aria-pressed={on}>
      {icon && <Icon name={icon} size={small ? 12 : 13} />}
      {children}
    </button>
  )
}

export function Dot({ tone, pulse, label }: { tone: string; pulse?: boolean; label?: string }) {
  return <span className={`dot ${tone}${pulse ? ' pulse' : ''}`} title={label} aria-label={label} role={label ? 'img' : undefined} />
}

export function SearchField({ value, onChange, placeholder, id }: { value: string; onChange: (v: string) => void; placeholder: string; id?: string }) {
  return (
    <label className="search no-drag">
      <Icon name="search" size={13} />
      <input
        id={id}
        type="search"
        spellCheck={false}
        placeholder={placeholder}
        aria-label={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            onChange('')
            e.currentTarget.blur()
          }
        }}
      />
    </label>
  )
}

export function Empty({ title, body, action, icon = true }: { title: string; body?: string; action?: ReactNode; icon?: boolean }) {
  return (
    <div className="empty">
      {icon && <img src={iconUrl} alt="" />}
      <div className="empty-title">{title}</div>
      {body && <div className="empty-body">{body}</div>}
      {action}
    </div>
  )
}

export function DialogLayer() {
  const dialog = useStore((s) => s.dialog)
  const dismiss = useStore((s) => s.dismissDialog)
  const [busy, setBusy] = useState(false)
  const confirmRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!dialog) return
    confirmRef.current?.focus()
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') dismiss()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [dialog, dismiss])
  const run = async (): Promise<void> => {
    if (!dialog || busy) return
    setBusy(true)
    try {
      await dialog.onConfirm()
    } finally {
      setBusy(false)
      dismiss()
    }
  }
  return (
    <AnimatePresence>
      {dialog && (
        <motion.div className="dialog-scrim" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={dismiss}>
          <motion.div
            className="dialog"
            initial={{ opacity: 0, scale: 0.97, y: 8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98 }}
            transition={{ type: 'spring', stiffness: 460, damping: 34 }}
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-modal
            aria-labelledby="dialog-title"
          >
            <div className="dialog-title" id="dialog-title">
              {dialog.title}
            </div>
            <div className="dialog-body">{dialog.body}</div>
            <div className="dialog-actions">
              <Btn kind="ghost" onClick={dismiss}>
                {tr('Cancel')}
              </Btn>
              <button ref={confirmRef} className={`sbtn sbtn-${dialog.danger ? 'danger' : 'primary'}`} onClick={() => void run()} disabled={busy}>
                {dialog.confirm}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}

export function ToastLayer() {
  const toast = useStore((s) => s.toast)
  const dismiss = useStore((s) => s.dismissToast)
  return (
    <AnimatePresence>
      {toast && (
        <motion.div
          key={toast.id}
          className="stoast"
          role="status"
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 6 }}
          transition={{ type: 'spring', stiffness: 500, damping: 36 }}
        >
          <span>{toast.text}</span>
          {toast.action && (
            <button
              onClick={() => {
                toast.action?.run()
                dismiss()
              }}
            >
              {toast.action.label}
            </button>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  )
}
