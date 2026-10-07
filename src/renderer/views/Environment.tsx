import { useEffect, useState } from 'react'
import type { Service, ServiceDetail } from '../../shared/types'
import { copy } from '../actions'
import { errorText } from '../shared/format'
import { tr } from '../shared/i18n'
import { useStore } from '../store'
import { Btn, Empty, Group, SearchField } from '../ui'

export function Environment({ s, detail }: { s: Service; detail: ServiceDetail | null }) {
  const flash = useStore((x) => x.flash)
  const [query, setQuery] = useState('')
  const [revealed, setRevealed] = useState<Record<string, string>>({})
  useEffect(() => setRevealed({}), [s.id])
  if (!detail) return null
  if (!detail.environment) return <Empty icon={false} title={tr("The environment isn't available for this service.")} />
  const q = query.trim().toLowerCase()
  const vars = detail.environment.filter((v) => !q || v.name.toLowerCase().includes(q) || (!v.secret && v.value.toLowerCase().includes(q)))
  const reveal = async (name: string): Promise<void> => {
    const r = await window.localhost.reveal(s.id, name)
    if (r.ok) setRevealed((x) => ({ ...x, [name]: r.value }))
    else flash(errorText(r.error))
  }
  const hide = (name: string): void =>
    setRevealed((x) => {
      const next = { ...x }
      delete next[name]
      return next
    })
  return (
    <Group
      head={
        <span className="group-actions" style={{ marginLeft: 0, flex: 1 }}>
          <SearchField value={query} onChange={setQuery} placeholder={tr('Filter variables')} />
        </span>
      }
      foot={tr('Values that look like secrets are hidden. Revealing one reads it from the running process.')}
    >
      {vars.length === 0 && <div className="idle-row">{tr('No variables.')}</div>}
      {vars.map((v) => {
        const shown = v.name in revealed
        return (
          <div key={v.name} className="row">
            <div className="row-text">
              <div className="env-name selectable">{v.name}</div>
              <div className="env-value">{shown ? revealed[v.name] : v.value}</div>
            </div>
            <div className="row-control">
              {v.secret && (
                <Btn small kind="ghost" onClick={() => (shown ? hide(v.name) : void reveal(v.name))}>
                  {shown ? tr('Hide') : tr('Reveal')}
                </Btn>
              )}
              {(!v.secret || shown) && (
                <Btn small kind="ghost" icon="copy" title={tr('Copy')} onClick={() => void copy(revealed[v.name] ?? v.value, tr('Copied {0}', v.name))} />
              )}
            </div>
          </div>
        )
      })}
    </Group>
  )
}
