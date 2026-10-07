import { useEffect, useState } from 'react'
import type { Service, ServiceDetail, Snapshot } from '../../shared/types'
import { copy, dismiss, openIn, openService, restart, start, stop } from '../actions'
import { address, bytes, cpu, healthText, live, protocolName, runtimeName, since, statusText, subtitle, tone } from '../shared/format'
import { useNow } from '../shared/hooks'
import { tr } from '../shared/i18n'
import { Icon } from '../shared/icons'
import { useStore, type Tab } from '../store'
import { Btn, Dot, Empty, Field, Group, Seg } from '../ui'
import { Environment } from './Environment'
import { Logs } from './Logs'
import { Processes } from './Processes'

const api = window.localhost

function useDetail(id: string, tab: Tab): ServiceDetail | null {
  const [detail, setDetail] = useState<ServiceDetail | null>(null)
  useEffect(() => {
    let alive = true
    setDetail(null)
    const load = (): void =>
      void api.inspect(id).then((r) => {
        if (alive && r.ok) setDetail(r.value)
      })
    load()
    const t = tab === 'processes' ? setInterval(load, 3000) : null
    return () => {
      alive = false
      if (t) clearInterval(t)
    }
  }, [id, tab])
  return detail
}

function Overview({ s, detail, snapshot, now }: { s: Service; detail: ServiceDetail | null; snapshot: Snapshot | null; now: number }) {
  const project = snapshot?.projects.find((p) => p.id === s.projectId)
  const on = live(s)
  const confidence = s.identity.confidence >= 0.8 ? tr('High confidence') : s.identity.confidence >= 0.6 ? tr('Likely') : tr('Possible')
  const evidence = (kind: string, detail: string): string =>
    ({
      command: tr('The command includes {0}', detail),
      executable: tr('The program is {0}', detail),
      port: tr('It listens on {0}, the usual port', detail),
      dependency: tr('The project depends on {0}', detail),
      container: tr('It runs the {0} image', detail),
      managed: tr('Localhost started it with {0}', detail),
    })[kind] ?? detail
  return (
    <>
      {s.ports.length > 0 && (
        <Group title={tr('Addresses')}>
          {s.ports.map((p) => (
            <div key={p.port} className="addr">
              <Dot tone={p.health.state === 'responding' ? 'green' : p.health.state === 'listening' ? 'gray' : 'red'} />
              <div className="addr-text">
                {p.url ? (
                  <button className="addr-link" onClick={() => openService(s, p.port)}>
                    {address(p)}
                  </button>
                ) : (
                  <span className="selectable">{address(p)}</span>
                )}
                <div className="row-desc">
                  {[protocolName(p.protocol), healthText(p.health), p.loopback ? null : tr('Reachable from your network')].filter(Boolean).join(' · ')}
                </div>
              </div>
              <Btn small kind="ghost" icon="copy" title={tr('Copy')} onClick={() => void copy(p.url ?? address(p))} />
            </div>
          ))}
        </Group>
      )}
      <Group title={tr('Details')}>
        {project && (
          <Field title={tr('Project')}>
            {project.name}
            {s.package ? ` / ${s.package}` : ''}
          </Field>
        )}
        {s.image && (
          <Field title={tr('Image')} mono>
            {s.image}
          </Field>
        )}
        {s.command && (
          <Field title={tr('Command')}>
            <div className="code selectable">{s.command}</div>
          </Field>
        )}
        {s.cwd && (
          <Field title={tr('Folder')} mono>
            {s.cwd}
          </Field>
        )}
        {detail?.executable && (
          <Field title={tr('Executable')} mono>
            {detail.executable}
          </Field>
        )}
        {s.startedAt && on && <Field title={tr('Started')}>{since(s.startedAt, now)}</Field>}
        {on && s.source !== 'container' && (
          <>
            <Field title={tr('CPU')}>{cpu(s.cpu)}</Field>
            <Field title={tr('Memory')}>{bytes(s.memory)}</Field>
            <Field title={tr('Processes')}>{s.processCount}</Field>
            <Field title={tr('PID')}>{s.pid}</Field>
          </>
        )}
        {s.identity.runtime && <Field title={tr('Runtime')}>{runtimeName(s.identity.runtime)}</Field>}
        {s.source === 'managed' && <Field title={tr('Started by')}>Localhost</Field>}
      </Group>
      <Group
        title={s.identity.name ? tr('Why Localhost thinks this is {0}', s.identity.name) : tr('How Localhost identified this')}
        foot={s.identity.tool ? confidence : undefined}
      >
        {s.identity.evidence.length ? (
          s.identity.evidence.map((e, i) => (
            <div key={i} className="evidence">
              <Icon name="check" size={13} />
              <span>{evidence(e.kind, e.detail)}</span>
            </div>
          ))
        ) : (
          <div className="idle-row">{tr('Localhost found an open port but no signs of a known tool.')}</div>
        )}
      </Group>
    </>
  )
}

function Banners({ s, snapshot }: { s: Service; snapshot: Snapshot | null }) {
  const open = useStore((x) => x.open)
  const owner = s.blockedPort ? snapshot?.services.find((o) => o.id !== s.id && o.ports.some((p) => p.port === s.blockedPort)) : undefined
  const shared = s.ports.find((p) => p.shared)
  return (
    <>
      {s.blockedPort ? (
        <div className="banner red" role="status">
          <Icon name="alert" size={16} />
          <div>
            <div className="row-title">{tr('Port {0} is already in use', s.blockedPort)}</div>
            <div className="row-desc">
              {owner
                ? tr('{0} could not start because {1} is using it.', s.title, owner.title)
                : tr('{0} could not start because another process is using the port.', s.title)}
            </div>
            {owner && (
              <div className="banner-actions">
                <Btn small onClick={() => open(owner.id)}>
                  {tr('Show {0}', owner.title)}
                </Btn>
                {owner.controllable && (
                  <Btn small onClick={() => stop(owner)}>
                    {tr('Stop {0}', owner.title)}
                  </Btn>
                )}
              </div>
            )}
          </div>
        </div>
      ) : shared ? (
        <div className="banner" role="status">
          <Icon name="alert" size={16} />
          <div className="row-desc">{tr('More than one service listens on port {0}.', shared.port)}</div>
        </div>
      ) : null}
      {!s.controllable && s.source === 'process' && (
        <div className="banner green">
          <Icon name="shield" size={16} />
          <div className="row-desc">{tr("Another user owns this process. Localhost can show it but can't control it without administrator rights.")}</div>
        </div>
      )}
    </>
  )
}

export function Detail({ id, snapshot, back }: { id: string; snapshot: Snapshot | null; back: string }) {
  const tab = useStore((x) => x.tab)
  const setTab = useStore((x) => x.setTab)
  const close = useStore((x) => x.close)
  const now = useNow(5000)
  const s = snapshot?.services.find((x) => x.id === id)
  const detail = useDetail(id, tab)
  useEffect(() => {
    if (s?.source === 'container' && tab === 'processes') setTab('overview')
  }, [s?.source, tab, setTab])
  if (!s) {
    return (
      <>
        <div className="detail-head">
          <div>
            <Btn small kind="ghost" icon="back" onClick={close}>
              {back}
            </Btn>
          </div>
        </div>
        <div className="content-body">
          <Empty icon={false} title={tr('This service is no longer running.')} />
        </div>
      </>
    )
  }
  const on = live(s)
  const tabs: { value: Tab; label: string }[] = [
    { value: 'overview', label: tr('Overview') },
    { value: 'logs', label: tr('Logs') },
    { value: 'environment', label: tr('Environment') },
    ...(s.source === 'container' ? [] : [{ value: 'processes' as Tab, label: tr('Processes') }]),
  ]
  return (
    <>
      <div className="detail-head">
        <div>
          <div className="back">
            <Btn small kind="ghost" icon="back" onClick={close}>
              {back}
            </Btn>
          </div>
          <h1 className="content-title selectable">{s.title}</h1>
          <div className="content-sub">{s.source === 'managed' && !on ? s.command : subtitle(s)}</div>
          <div className="detail-status">
            <Dot tone={tone(s)} pulse={s.status === 'starting' || s.status === 'stopping'} />
            {statusText(s, now)}
          </div>
        </div>
        <div className="detail-bar">
          {s.ports.some((p) => p.url) && (
            <Btn kind="primary" icon="jump" onClick={() => openService(s)}>
              {tr('Open')}
            </Btn>
          )}
          {s.controllable &&
            (s.source === 'managed' && !on ? (
              <>
                <Btn icon="play" onClick={() => start(s)}>
                  {tr('Start')}
                </Btn>
                <Btn kind="ghost" onClick={() => dismiss(s)}>
                  {tr('Remove')}
                </Btn>
              </>
            ) : (
              <>
                {(s.source !== 'process' || s.projectId) && (
                  <Btn icon="refresh" onClick={() => restart(s)}>
                    {tr('Restart')}
                  </Btn>
                )}
                <Btn icon="stop" onClick={() => stop(s)}>
                  {tr('Stop')}
                </Btn>
              </>
            ))}
          {(s.source !== 'container' || s.projectId) && (
            <Btn kind="ghost" icon="terminal" title={tr('Open in terminal')} onClick={() => openIn('terminal', 'service', s.id)} />
          )}
          <Btn kind="ghost" icon="more" title={tr('More actions')} onClick={() => api.serviceMenu(s.id)} />
          <Seg label={s.title} value={tab} onChange={setTab} options={tabs} />
        </div>
      </div>
      {tab === 'logs' ? (
        <Logs key={s.id} s={s} />
      ) : (
        <div className="content-body">
          <Banners s={s} snapshot={snapshot} />
          {tab === 'overview' && <Overview s={s} detail={detail} snapshot={snapshot} now={now} />}
          {tab === 'environment' && <Environment s={s} detail={detail} />}
          {tab === 'processes' && <Processes detail={detail} />}
        </div>
      )}
    </>
  )
}
