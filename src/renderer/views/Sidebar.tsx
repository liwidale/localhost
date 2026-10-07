import type { Settings, Snapshot } from '../../shared/types'
import { addProject } from '../actions'
import { live, tone } from '../shared/format'
import { tr } from '../shared/i18n'
import { Icon, type IconName } from '../shared/icons'
import { useStore, type Page } from '../store'
import { Dot } from '../ui'
import { projectServices } from '../view'

function Item({ icon, label, count, on, onClick, dot }: { icon?: IconName; label: string; count?: number; on: boolean; onClick: () => void; dot?: string }) {
  return (
    <button className={`nav-item${on ? ' on' : ''}`} onClick={onClick} aria-current={on ? 'page' : undefined}>
      {icon ? <Icon name={icon} size={15} /> : <Dot tone={dot ?? 'gray'} />}
      <span className="nav-name">{label}</span>
      {count !== undefined && count > 0 && <span className="nav-count">{count}</span>}
    </button>
  )
}

export function Sidebar({ snapshot, settings }: { snapshot: Snapshot | null; settings: Settings | null }) {
  const page = useStore((s) => s.page)
  const go = useStore((s) => s.go)
  const is = (p: Page): boolean => page.kind === p.kind && (p.kind !== 'project' || (page.kind === 'project' && page.id === p.id))
  const visible = (snapshot?.services ?? []).filter((s) => settings?.showSystem || !s.system)
  const other = visible.filter((s) => !s.projectId)
  const ports = visible.reduce((n, s) => n + s.ports.length, 0)
  const projectTone = (id: string): string => {
    const list = projectServices(snapshot, id).filter(live)
    if (!list.length) return 'gray'
    const tones = list.map(tone)
    return tones.includes('red') ? 'red' : tones.includes('amber') ? 'amber' : 'green'
  }
  return (
    <aside className="sidebar">
      <div className="drag" />
      <div className="sidebar-scroll scroll">
        <nav aria-label={tr('Views')}>
          <Item icon="layers" label={tr('All services')} count={visible.filter(live).length} on={is({ kind: 'all' })} onClick={() => go({ kind: 'all' })} />
          <Item icon="globe" label={tr('Ports')} count={ports} on={is({ kind: 'ports' })} onClick={() => go({ kind: 'ports' })} />
        </nav>
        {!!snapshot?.projects.length && (
          <nav aria-label={tr('Projects')}>
            <div className="nav-label">{tr('Projects')}</div>
            {snapshot.projects.map((p) => (
              <Item
                key={p.id}
                label={p.name}
                dot={projectTone(p.id)}
                count={projectServices(snapshot, p.id).filter(live).length}
                on={is({ kind: 'project', id: p.id })}
                onClick={() => go({ kind: 'project', id: p.id })}
              />
            ))}
            {other.length > 0 && (
              <Item icon="sessions" label={tr('Other services')} count={other.length} on={is({ kind: 'other' })} onClick={() => go({ kind: 'other' })} />
            )}
          </nav>
        )}
        <nav className="nav-bottom">
          <Item icon="plus" label={tr('Add project')} on={false} onClick={() => void addProject()} />
          <Item icon="gear" label={tr('Settings')} on={is({ kind: 'settings' })} onClick={() => go({ kind: 'settings' })} />
        </nav>
      </div>
    </aside>
  )
}
