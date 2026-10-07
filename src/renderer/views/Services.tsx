import type { Project, Service, Settings, Snapshot } from '../../shared/types'
import { addProject, openService, toggleSystem } from '../actions'
import { address, bytes, cpu, live, subtitle, tone } from '../shared/format'
import { tr } from '../shared/i18n'
import { Icon } from '../shared/icons'
import { useStore } from '../store'
import { Btn, Dot, Empty, Group } from '../ui'
import { freePort, group, type Section } from '../view'

const api = window.localhost

export function ServiceRow({ s }: { s: Service }) {
  const open = useStore((x) => x.open)
  const on = live(s)
  return (
    <div
      className={`svc${on ? '' : ' dim'}`}
      role="button"
      tabIndex={0}
      data-id={s.id}
      onClick={() => open(s.id)}
      onKeyDown={(e) => e.key === 'Enter' && open(s.id)}
      onContextMenu={(e) => {
        e.preventDefault()
        api.serviceMenu(s.id)
      }}
    >
      <Dot tone={tone(s)} pulse={s.status === 'starting' || s.status === 'stopping'} />
      <div className="svc-name">
        <span className="svc-title">{s.title}</span>
        <span className="svc-desc">{subtitle(s)}</span>
      </div>
      <div className="svc-ports">
        {s.ports.slice(0, 2).map((p) =>
          p.url ? (
            <button
              key={p.port}
              className={`port${p.shared ? ' warn' : ''}`}
              title={p.url}
              onClick={(e) => {
                e.stopPropagation()
                openService(s, p.port)
              }}
            >
              {address(p)}
            </button>
          ) : (
            <span key={p.port} className={`port${p.shared ? ' warn' : ''}`}>
              {address(p)}
            </span>
          ),
        )}
        {s.ports.length > 2 && <span className="port">+{s.ports.length - 2}</span>}
      </div>
      <span className="svc-metric">{on && s.source !== 'container' ? cpu(s.cpu) : ''}</span>
      <span className="svc-metric mem">{on && s.memory ? bytes(s.memory) : ''}</span>
      <Icon name="forward" size={13} />
    </div>
  )
}

export function ProjectActions({ project }: { project: Project }) {
  return (
    <>
      {project.scripts.length > 0 && (
        <Btn small icon="play" onClick={() => api.scriptsMenu(project.id)}>
          {tr('Start')}
        </Btn>
      )}
      <Btn small kind="ghost" icon="more" title={tr('More actions')} onClick={() => api.projectMenu(project.id)} />
    </>
  )
}

function ProjectGroup({ section, heading }: { section: Section; heading: boolean }) {
  const p = section.project
  return (
    <Group
      title={heading ? (p?.name ?? '') : undefined}
      head={
        heading && p ? (
          <>
            <span className="group-path" title={p.path}>
              {p.path}
            </span>
            <span className="group-actions">
              <ProjectActions project={p} />
            </span>
          </>
        ) : undefined
      }
    >
      {section.services.length ? (
        section.services.map((s) => <ServiceRow key={s.id} s={s} />)
      ) : (
        <div className="idle-row">{tr('Nothing running. Start a script to see it here.')}</div>
      )}
    </Group>
  )
}

function NoMatch({ query }: { query: string }) {
  const port = freePort(query)
  return port !== null ? (
    <Empty icon={false} title={tr('Port {0} is free', port)} body={tr('Nothing on this computer is listening on it.')} />
  ) : (
    <Empty icon={false} title={tr('Nothing matches “{0}”', query)} />
  )
}

export function AllServices({ snapshot, settings, query }: { snapshot: Snapshot | null; settings: Settings | null; query: string }) {
  const g = group(snapshot, settings, query)
  if (!snapshot) return <Empty title={tr('Looking for services')} />
  const empty = !g.sections.length && !g.other.length && !g.system.length
  if (empty && query) return <NoMatch query={query} />
  if (empty && !g.hidden)
    return (
      <Empty
        title={tr('Nothing is running yet')}
        body={tr(
          'Start a dev server, database or container and it appears here, grouped by project. You can also add a project folder to start its scripts from Localhost.',
        )}
        action={
          <Btn kind="primary" icon="plus" onClick={() => void addProject()}>
            {tr('Add project')}
          </Btn>
        }
      />
    )
  return (
    <>
      {g.sections.map((section) => (
        <ProjectGroup key={section.project?.id} section={section} heading />
      ))}
      {g.other.length > 0 && (
        <Group title={g.sections.length ? tr('Other services') : undefined}>
          {g.other.map((s) => (
            <ServiceRow key={s.id} s={s} />
          ))}
        </Group>
      )}
      {g.system.length > 0 && (
        <Group title={tr('System')}>
          {g.system.map((s) => (
            <ServiceRow key={s.id} s={s} />
          ))}
        </Group>
      )}
      <SystemNote hidden={g.hidden} shown={Boolean(settings?.showSystem)} />
    </>
  )
}

function SystemNote({ hidden, shown }: { hidden: number; shown: boolean }) {
  if (!hidden && !shown) return null
  return (
    <div className="group-foot">
      {shown ? tr('System services are visible.') : tr('{0} system service is hidden.|{0} system services are hidden.', hidden)}
      <button onClick={() => void toggleSystem()}>{shown ? tr('Hide') : tr('Show')}</button>
    </div>
  )
}

export function ProjectPage({ snapshot, settings, query, id }: { snapshot: Snapshot | null; settings: Settings | null; query: string; id: string }) {
  const g = group(snapshot, settings, query)
  const section = g.sections.find((x) => x.project?.id === id)
  if (!section) return query ? <NoMatch query={query} /> : <Empty icon={false} title={tr('This project has nothing running.')} />
  return <ProjectGroup section={section} heading={false} />
}

export function OtherPage({ snapshot, settings, query }: { snapshot: Snapshot | null; settings: Settings | null; query: string }) {
  const all = group(snapshot, settings, query)
  const g = { ...all, other: all.other.filter((s) => !s.projectId), system: all.system.filter((s) => !s.projectId) }
  if (!g.other.length && !g.system.length) return query ? <NoMatch query={query} /> : <Empty icon={false} title={tr('Nothing else is running.')} />
  return (
    <>
      {g.other.length > 0 && (
        <Group>
          {g.other.map((s) => (
            <ServiceRow key={s.id} s={s} />
          ))}
        </Group>
      )}
      {g.system.length > 0 && (
        <Group title={tr('System')}>
          {g.system.map((s) => (
            <ServiceRow key={s.id} s={s} />
          ))}
        </Group>
      )}
      <SystemNote hidden={g.hidden} shown={Boolean(settings?.showSystem)} />
    </>
  )
}
