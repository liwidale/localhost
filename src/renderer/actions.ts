import type { Project, Result, Service, Snapshot, Target } from '../shared/types'
import { address, errorText } from './shared/format'
import { tr } from './shared/i18n'
import { store } from './store'

const api = window.localhost

async function attempt<T>(run: () => Promise<Result<T>>): Promise<T | undefined> {
  const r = await run()
  if (r.ok) return r.value
  store().flash(errorText(r.error))
  return undefined
}

export async function copy(text: string, message?: string): Promise<void> {
  await api.copyText(text)
  store().flash(message ?? tr('Copied {0}', text.length > 60 ? `${text.slice(0, 57)}…` : text))
}

export function openService(s: Service, port?: number): void {
  const target = port ?? s.ports.find((p) => p.url)?.port
  if (target !== undefined) void attempt(() => api.open(s.id, target))
}

export function copyAddress(s: Service): void {
  const p = s.ports.find((x) => x.url) ?? s.ports[0]
  if (p) void copy(p.url ?? address(p))
}

export function stop(s: Service, force = false): void {
  const run = async (): Promise<void> => {
    const r = await api.stop(s.id, force)
    if (!r.ok) return store().flash(errorText(r.error))
    if (s.source === 'managed' && !force) store().flash(tr('Stopped {0}', s.title), { label: tr('Undo'), run: () => void attempt(() => api.restart(s.id)) })
  }
  if (force) {
    store().confirm({
      title: tr('Force quit {0}?', s.title),
      body: tr(
        'The process ends immediately. Unsaved work in it is lost.|Its {0} processes end immediately. Unsaved work in them is lost.',
        Math.max(1, s.processCount),
      ),
      confirm: tr('Force quit'),
      danger: true,
      onConfirm: run,
    })
  } else if (s.source === 'managed') {
    void run()
  } else {
    store().confirm({
      title: tr('Stop {0}?', s.title),
      body:
        s.source === 'container'
          ? tr('Docker stops the container. Its volumes and data stay.')
          : api.platform === 'win32'
            ? tr('Localhost sends it Ctrl+C, the same as pressing it in the terminal that started it.')
            : tr('Localhost sends it an interrupt, the same as pressing Control-C in the terminal that started it.'),
      confirm: tr('Stop'),
      onConfirm: run,
    })
  }
}

export function restart(s: Service): void {
  const run = async (): Promise<void> => {
    const id = await attempt(() => api.restart(s.id))
    if (id && id !== s.id) store().open(id, 'logs')
  }
  if (s.source === 'process') {
    store().confirm({
      title: tr('Restart {0} in Localhost?', s.title),
      body: tr(
        'Localhost stops it and runs the same command in the same folder, so its output appears in Logs. The terminal that started it returns to its prompt.',
      ),
      confirm: tr('Restart'),
      onConfirm: run,
    })
  } else void run()
}

export function start(s: Service): void {
  void attempt(() => api.restart(s.id))
}

export function dismiss(s: Service): void {
  void attempt(async () => {
    const r = await api.dismiss(s.id)
    if (r.ok && store().detail === s.id) store().close()
    return r
  })
}

export async function runScript(project: Project, scriptId: string, trust = false): Promise<void> {
  const r = await api.startScript(project.id, scriptId, trust)
  if (r.ok) return store().open(r.value, 'logs')
  if (r.error.code === 'needsTrust') {
    const { command, path } = r.error
    store().confirm({
      title: tr('Run scripts from {0}?', project.name),
      body: tr('Localhost will run {0} in {1}. Scripts can run any code, so only run scripts from projects you trust.', command, path),
      confirm: tr('Run'),
      onConfirm: () => runScript(project, scriptId, true),
    })
  } else store().flash(errorText(r.error))
}

export function openIn(kind: 'folder' | 'terminal' | 'editor', target: Target, id: string): void {
  const call = { folder: api.openFolder, terminal: api.openTerminal, editor: api.openEditor }[kind]
  void attempt(() => call(target, id))
}

export function stopProject(project: Project): void {
  void attempt(() => api.stopProject(project.id))
}

export function keepProject(project: Project): void {
  void attempt(() => api.keepProject(project.id))
}

export function removeProject(project: Project): void {
  store().confirm({
    title: tr('Remove {0} from Localhost?', project.name),
    body: tr('It still appears while something in it is running. Nothing on disk changes.'),
    confirm: tr('Remove'),
    onConfirm: async () => {
      await attempt(() => api.removeProject(project.id))
      const page = store().page
      if (page.kind === 'project' && page.id === project.id) store().go({ kind: 'all' })
    },
  })
}

export async function addProject(): Promise<void> {
  const id = await attempt(() => api.addProject())
  if (id) store().go({ kind: 'project', id })
}

export async function toggleSystem(): Promise<void> {
  const s = await api.settings()
  await api.setSettings({ showSystem: !s.showSystem })
}

export function handleMenu(snapshot: Snapshot | null, action: string, target: string | null, extra: string | null): void {
  switch (action) {
    case 'refresh':
      return api.refresh()
    case 'find':
      return document.getElementById('search')?.focus()
    case 'settings':
      return store().go({ kind: 'settings' })
    case 'add-project':
      return void addProject()
    case 'toggle-system':
      return void toggleSystem()
  }
  const project = snapshot?.projects.find((p) => p.id === target)
  if (project) {
    if (action === 'run' && extra) return void runScript(project, extra)
    if (action === 'folder' || action === 'terminal' || action === 'editor') return openIn(action, 'project', project.id)
    if (action === 'stop-all') return stopProject(project)
    if (action === 'keep') return keepProject(project)
    if (action === 'remove') return removeProject(project)
    return
  }
  const s = snapshot?.services.find((x) => x.id === target)
  if (!s) return
  switch (action) {
    case 'open':
      return openService(s)
    case 'copy':
      return copyAddress(s)
    case 'logs':
      return store().open(s.id, 'logs')
    case 'start':
      return start(s)
    case 'dismiss':
      return dismiss(s)
    case 'restart':
      return restart(s)
    case 'stop':
      return stop(s)
    case 'kill':
      return stop(s, true)
    case 'folder':
    case 'terminal':
    case 'editor':
      return openIn(action, 'service', s.id)
  }
}
