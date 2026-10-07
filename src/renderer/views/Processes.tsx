import type { ServiceDetail } from '../../shared/types'
import { bytes, cpu } from '../shared/format'
import { tr } from '../shared/i18n'
import { Empty, Group } from '../ui'

export function Processes({ detail }: { detail: ServiceDetail | null }) {
  if (!detail) return null
  if (!detail.tree.length) return <Empty icon={false} title={tr('No process information is available.')} />
  const chain = [...detail.ancestry].reverse()
  return (
    <>
      {chain.length > 0 && (
        <div className="chain">
          <span>{tr('Started from')}</span>
          {chain.map((n, i) => (
            <span key={n.pid} style={{ display: 'contents' }}>
              <span className="crumb" title={n.command}>
                {n.name}
              </span>
              {i < chain.length - 1 && <span>›</span>}
            </span>
          ))}
        </div>
      )}
      <Group>
        {detail.tree.map((n) => (
          <div key={n.pid} className="proc" style={{ paddingLeft: 14 + Math.min(n.depth, 8) * 16 }}>
            <div className="proc-head">
              <span className="proc-name">{n.name}</span>
              <span className="muted num">{n.pid}</span>
              {n.ports.length > 0 && <span className="port">{tr('Ports {0}', n.ports.join(', '))}</span>}
              <span className="proc-meta">
                {cpu(n.cpu)} · {bytes(n.memory)}
              </span>
            </div>
            <div className="proc-cmd selectable" title={n.command}>
              {n.command}
            </div>
          </div>
        ))}
      </Group>
    </>
  )
}
