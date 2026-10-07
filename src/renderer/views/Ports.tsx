import type { Settings, Snapshot } from '../../shared/types'
import { openService } from '../actions'
import { healthText, protocolName, tone } from '../shared/format'
import { tr } from '../shared/i18n'
import { useStore } from '../store'
import { Dot, Empty, Group } from '../ui'
import { freePort, portRows } from '../view'

export function Ports({ snapshot, settings, query }: { snapshot: Snapshot | null; settings: Settings | null; query: string }) {
  const open = useStore((s) => s.open)
  const rows = portRows(snapshot, settings, query)
  if (!rows.length) {
    const port = freePort(query)
    if (port !== null) return <Empty icon={false} title={tr('Port {0} is free', port)} body={tr('Nothing on this computer is listening on it.')} />
    return <Empty icon={false} title={query ? tr('Nothing matches “{0}”', query) : tr('No open ports.')} />
  }
  return (
    <Group>
      <div className="ports-head">
        <span>{tr('Port')}</span>
        <span>{tr('Service')}</span>
        <span>{tr('Project')}</span>
        <span>{tr('Address')}</span>
        <span>{tr('PID')}</span>
        <span>{tr('Status')}</span>
      </div>
      {rows.map(({ port, service, project }) => (
        <div
          key={`${service.id}:${port.port}`}
          className="ports-row"
          role="button"
          tabIndex={0}
          onClick={() => open(service.id)}
          onDoubleClick={() => openService(service, port.port)}
          onKeyDown={(e) => e.key === 'Enter' && open(service.id)}
          onContextMenu={(e) => {
            e.preventDefault()
            window.localhost.serviceMenu(service.id)
          }}
        >
          <span className="p">{port.port}</span>
          <span className="svc-cell">
            <Dot tone={tone(service)} />
            {service.title}
          </span>
          <span className="muted">{project?.name ?? ''}</span>
          <span className="mono muted">{port.address}</span>
          <span className="muted num">{service.pid ?? ''}</span>
          <span className="muted">
            {protocolName(port.protocol)} · {healthText(port.health)}
            {port.shared ? ` · ${tr('shared')}` : ''}
          </span>
        </div>
      ))}
    </Group>
  )
}
