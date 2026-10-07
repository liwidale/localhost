import type { Port, Project, Service, Settings, Snapshot } from '../shared/types'

export function matches(s: Service, query: string, projectName?: string): boolean {
  const q = query.trim().toLowerCase().replace(/^:/, '')
  if (!q) return true
  if (/^\d+$/.test(q)) return s.ports.some((p) => String(p.port).startsWith(q)) || String(s.pid) === q
  return [s.title, s.identity.name, s.identity.runtime, s.package, s.command, s.image, projectName, ...s.ports.map((p) => `localhost:${p.port}`)].some((v) =>
    v?.toLowerCase().includes(q),
  )
}

const numeric = (query: string): boolean => /^:?\d+$/.test(query.trim())

export interface Section {
  project: Project | null
  services: Service[]
}

export interface Grouped {
  sections: Section[]
  other: Service[]
  system: Service[]
  hidden: number
}

export function group(snapshot: Snapshot | null, settings: Settings | null, query: string): Grouped {
  const out: Grouped = { sections: [], other: [], system: [], hidden: 0 }
  if (!snapshot) return out
  const showSystem = Boolean(settings?.showSystem) || numeric(query)
  const names = new Map(snapshot.projects.map((p) => [p.id, p.name]))
  const services = snapshot.services.filter((s) => matches(s, query, s.projectId ? names.get(s.projectId) : undefined))
  const q = query.trim().toLowerCase()
  for (const project of snapshot.projects) {
    const own = services.filter((s) => s.projectId === project.id && (showSystem || !s.system))
    const named = !q || project.name.toLowerCase().includes(q)
    if (own.length || (project.saved && named)) out.sections.push({ project, services: own })
  }
  const shown = new Set(out.sections.map((g) => g.project?.id))
  for (const s of services) {
    if (s.projectId && shown.has(s.projectId)) continue
    if (s.system) {
      if (showSystem) out.system.push(s)
      else out.hidden++
    } else out.other.push(s)
  }
  return out
}

export interface PortRow {
  port: Port
  service: Service
  project: Project | undefined
}

export function portRows(snapshot: Snapshot | null, settings: Settings | null, query: string): PortRow[] {
  if (!snapshot) return []
  const projects = new Map(snapshot.projects.map((p) => [p.id, p]))
  const showSystem = Boolean(settings?.showSystem) || numeric(query)
  const q = query.trim().replace(/^:/, '')
  const rows: PortRow[] = []
  for (const service of snapshot.services) {
    if (service.system && !showSystem) continue
    const project = service.projectId ? projects.get(service.projectId) : undefined
    if (!matches(service, query, project?.name)) continue
    for (const port of service.ports) {
      if (/^\d+$/.test(q) && !String(port.port).startsWith(q)) continue
      rows.push({ port, service, project })
    }
  }
  return rows.sort((a, b) => a.port.port - b.port.port)
}

export function freePort(query: string): number | null {
  const q = query.trim().replace(/^:/, '')
  if (!/^\d{2,5}$/.test(q)) return null
  const n = Number(q)
  return n > 0 && n < 65536 ? n : null
}

export const projectServices = (snapshot: Snapshot | null, id: string): Service[] => snapshot?.services.filter((s) => s.projectId === id) ?? []
