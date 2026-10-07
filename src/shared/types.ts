export type Confidence = 'low' | 'medium' | 'high'
export type Source = 'process' | 'managed' | 'container'
export type Status = 'running' | 'starting' | 'stopping' | 'stopped' | 'exited' | 'crashed'
export type HealthState = 'listening' | 'responding' | 'error' | 'unreachable'
export type Protocol = 'http' | 'https' | 'postgres' | 'mySql' | 'redis' | 'mongo' | 'amqp' | 'smtp' | 'tcp'
export type Kind =
  | 'devServer'
  | 'webServer'
  | 'database'
  | 'cache'
  | 'queue'
  | 'search'
  | 'mail'
  | 'storage'
  | 'debugger'
  | 'task'
  | 'proxy'
  | 'notebook'
  | 'emulator'
  | 'portForward'
  | 'process'
export type EvidenceKind = 'command' | 'executable' | 'port' | 'dependency' | 'container' | 'managed'

export interface Script {
  id: string
  name: string
  package: string
  command: string
  longRunning: boolean
}

export interface Project {
  id: string
  name: string
  path: string
  ecosystems: string[]
  packageManager: string | null
  scripts: Script[]
  saved: boolean
  trusted: boolean
  confidence: Confidence
}

export interface Health {
  state: HealthState
  httpStatus: number | null
  latencyMs: number | null
  checkedAt: number | null
}

export interface Port {
  port: number
  address: string
  loopback: boolean
  protocol: Protocol
  health: Health
  url: string | null
  shared: boolean
}

export interface Evidence {
  kind: EvidenceKind
  detail: string
}

export interface Identity {
  tool: string | null
  name: string | null
  kind: Kind
  runtime: string | null
  confidence: number
  evidence: Evidence[]
}

export interface Service {
  id: string
  source: Source
  projectId: string | null
  package: string | null
  title: string
  identity: Identity
  status: Status
  ports: Port[]
  pid: number | null
  processCount: number
  cpu: number
  memory: number
  startedAt: number | null
  command: string | null
  cwd: string | null
  image: string | null
  system: boolean
  controllable: boolean
  hasLogs: boolean
  exitCode: number | null
  blockedPort: number | null
}

export interface Snapshot {
  projects: Project[]
  services: Service[]
  docker: 'unavailable' | 'running'
  platform: string
}

export interface ProcessNode {
  pid: number
  depth: number
  name: string
  command: string
  cpu: number
  memory: number
  ports: number[]
}

export interface EnvVar {
  name: string
  value: string
  secret: boolean
}

export interface ServiceDetail {
  id: string
  tree: ProcessNode[]
  ancestry: ProcessNode[]
  environment: EnvVar[] | null
  executable: string | null
}

export type Theme = 'system' | 'light' | 'dark'

export interface Settings {
  showSystem: boolean
  healthChecks: boolean
  notifications: boolean
  theme: Theme
  motion: 'full' | 'reduced'
  language: string
}

export interface Span {
  t: string
  fg?: string
  b?: boolean
  d?: boolean
  i?: boolean
  u?: boolean
}

export interface LogLine {
  seq: number
  ts: number
  stream: 'out' | 'err' | 'system'
  level: 'info' | 'warn' | 'error'
  text: string
  spans?: Span[]
}

export type ActionError =
  | { code: 'notFound' }
  | { code: 'protected' }
  | { code: 'denied' }
  | { code: 'unsupported' }
  | { code: 'failed'; message: string }
  | { code: 'needsTrust'; command: string; path: string }
  | { code: 'notFoundProgram'; program: string }
  | { code: 'invalidFolder' }

export type Result<T = null> = { ok: true; value: T } | { ok: false; error: ActionError }

export type EngineEvent =
  | { type: 'crashed'; id: string; title: string; code: number | null }
  | { type: 'portInUse'; id: string; title: string; port: number; owner: string | null }
  | { type: 'stopTimedOut'; id: string; title: string }

export interface MenuAction {
  action: string
  target: string | null
  extra: string | null
}

export type Target = 'project' | 'service'
