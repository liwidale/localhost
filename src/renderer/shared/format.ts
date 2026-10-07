import type { ActionError, Health, Kind, Port, Protocol, Service } from '../../shared/types'
import { tr } from './i18n'

export function bytes(n: number): string {
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`
  if (n < 1024 * 1024 * 1024) return `${Math.round(n / (1024 * 1024))} MB`
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

export function cpu(n: number): string {
  if (n < 0.05) return '0%'
  return `${n < 10 ? n.toFixed(1) : Math.round(n)}%`
}

export function duration(seconds: number): string {
  if (seconds < 60) return tr('{0} s', Math.max(1, Math.floor(seconds)))
  if (seconds < 3600) return tr('{0} min', Math.floor(seconds / 60))
  if (seconds < 86400) return tr('{0} h', Math.floor(seconds / 3600))
  return tr('{0} d', Math.floor(seconds / 86400))
}

export function since(unixSeconds: number, now = Date.now()): string {
  const s = now / 1000 - unixSeconds
  return s < 10 ? tr('now') : tr('{0} ago', duration(s))
}

export function clock(ms: number): string {
  const d = new Date(ms)
  const pad = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`
}

const KINDS: Record<Kind, () => string> = {
  devServer: () => tr('Dev server'),
  webServer: () => tr('Web server'),
  database: () => tr('Database'),
  cache: () => tr('Cache'),
  queue: () => tr('Message queue'),
  search: () => tr('Search engine'),
  mail: () => tr('Mail catcher'),
  storage: () => tr('Object storage'),
  debugger: () => tr('Debugger'),
  task: () => tr('Background task'),
  proxy: () => tr('Proxy'),
  notebook: () => tr('Notebook'),
  emulator: () => tr('Emulator'),
  portForward: () => tr('Port forward'),
  process: () => tr('Process'),
}

export const kindName = (k: Kind): string => KINDS[k]()

const RUNTIMES: Record<string, string> = {
  node: 'Node.js',
  bun: 'Bun',
  deno: 'Deno',
  python: 'Python',
  ruby: 'Ruby',
  java: 'Java',
  dotnet: '.NET',
  php: 'PHP',
  erlang: 'Erlang',
  go: 'Go',
  rust: 'Rust',
  docker: 'Docker',
  wsl: 'WSL',
}

export const runtimeName = (r: string | null): string | null => (r ? RUNTIMES[r] || r : null)

const PROTOCOLS: Record<Protocol, string> = {
  http: 'HTTP',
  https: 'HTTPS',
  postgres: 'PostgreSQL',
  mySql: 'MySQL',
  redis: 'Redis',
  mongo: 'MongoDB',
  amqp: 'AMQP',
  smtp: 'SMTP',
  tcp: 'TCP',
}

export const protocolName = (p: Protocol): string => PROTOCOLS[p]

export const live = (s: Service): boolean => s.pid !== null || s.source === 'container'

export function statusName(s: Service): string {
  return {
    running: tr('Running'),
    starting: tr('Starting'),
    stopping: tr('Stopping'),
    stopped: tr('Stopped'),
    exited: tr('Exited'),
    crashed: tr('Crashed'),
  }[s.status]
}

export function subtitle(s: Service): string {
  if (s.source === 'managed' && !live(s)) return statusText(s)
  const kind = kindName(s.identity.kind)
  if (s.source === 'container') return [s.identity.name ?? s.image, s.identity.name ? kind : null].filter(Boolean).join(' · ')
  if (s.identity.name) return s.identity.kind === 'process' ? s.identity.name : `${s.identity.name} · ${kind}`
  const runtime = runtimeName(s.identity.runtime)
  return runtime ? tr('{0} process', runtime) : kind
}

export type Tone = 'green' | 'amber' | 'red' | 'gray'

export function tone(s: Service): Tone {
  if (s.status === 'crashed') return 'red'
  if (s.status === 'stopped' || s.status === 'exited') return 'gray'
  if (s.status === 'starting' || s.status === 'stopping') return 'amber'
  if (s.blockedPort) return 'red'
  if (s.ports.some((p) => p.health.state === 'error' || p.health.state === 'unreachable')) return 'red'
  if (s.ports.some((p) => p.shared)) return 'amber'
  return 'green'
}

export function healthText(h: Health): string {
  const latency = h.latencyMs === null ? null : h.latencyMs < 1 ? tr('under 1 ms') : tr('{0} ms', h.latencyMs)
  switch (h.state) {
    case 'responding':
      return [h.httpStatus ? `HTTP ${h.httpStatus}` : tr('Responding'), latency].filter(Boolean).join(' · ')
    case 'error':
      return h.httpStatus ? `HTTP ${h.httpStatus}` : tr('Not healthy')
    case 'unreachable':
      return tr('Not responding')
    default:
      return tr('Listening')
  }
}

export function statusText(s: Service, now = Date.now()): string {
  if (s.status === 'crashed' || s.status === 'exited') return s.exitCode !== null ? tr('Exited with code {0}', s.exitCode) : statusName(s)
  if (s.status !== 'running') return statusName(s)
  const health = s.ports.map((p) => p.health).find((h) => h.state !== 'listening')
  if (health) return `${tr('Running')} · ${healthText(health)}`
  if (s.startedAt) return `${tr('Running')} · ${tr('for {0}', duration(now / 1000 - s.startedAt))}`
  return tr('Running')
}

export function address(p: Port): string {
  if (p.url) return p.url.replace(/^https?:\/\//, '')
  const host = p.loopback || p.address === '0.0.0.0' || p.address === '[::]' ? 'localhost' : p.address
  return `${host}:${p.port}`
}

export function errorText(e: ActionError | undefined): string {
  if (!e) return tr('Something went wrong. Try again.')
  switch (e.code) {
    case 'notFound':
      return tr('That service is no longer running.')
    case 'protected':
      return tr("Localhost can't control this process. It belongs to another user or to Localhost itself.")
    case 'denied':
      return tr("The system didn't allow it. The process may belong to another user.")
    case 'unsupported':
      return tr("This isn't available for this service.")
    case 'failed':
      return tr('Something went wrong: {0}', e.message)
    case 'notFoundProgram':
      return tr("Localhost couldn't find {0}. Check that it's installed and on your PATH.", e.program)
    case 'invalidFolder':
      return tr('Choose a project folder rather than your home folder or a system folder.')
    default:
      return tr('Something went wrong. Try again.')
  }
}
