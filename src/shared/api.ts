import type { EngineEvent, LogLine, MenuAction, Result, ServiceDetail, Settings, Snapshot, Target } from './types'

export interface LocalhostApi {
  platform: 'darwin' | 'win32' | 'linux'
  language: string
  version: string
  snapshot(): Promise<Snapshot | null>
  onSnapshot(cb: (s: Snapshot) => void): () => void
  onEvent(cb: (e: EngineEvent) => void): () => void
  onMenu(cb: (a: MenuAction) => void): () => void
  onQuitRequest(cb: (running: number) => void): () => void
  settings(): Promise<Settings>
  setSettings(patch: Partial<Settings>): Promise<Settings>
  onSettings(cb: (s: Settings) => void): () => void
  theme(): Promise<'light' | 'dark'>
  onTheme(cb: (t: 'light' | 'dark') => void): () => void
  editor(): Promise<string | null>

  inspect(id: string): Promise<Result<ServiceDetail>>
  reveal(id: string, name: string): Promise<Result<string>>
  stop(id: string, force: boolean): Promise<Result>
  restart(id: string): Promise<Result<string>>
  stopProject(projectId: string): Promise<Result>
  startScript(projectId: string, script: string, trust: boolean): Promise<Result<string>>
  dismiss(id: string): Promise<Result>
  clearLogs(id: string): Promise<Result>
  subscribeLogs(id: string, cb: (lines: LogLine[]) => void): Promise<() => void>
  open(id: string, port: number): Promise<Result>
  openFolder(target: Target, id: string): Promise<Result>
  openTerminal(target: Target, id: string): Promise<Result>
  openEditor(target: Target, id: string): Promise<Result>
  addProject(): Promise<Result<string | null>>
  keepProject(projectId: string): Promise<Result<string>>
  removeProject(projectId: string): Promise<Result>
  serviceMenu(id: string): void
  projectMenu(id: string): void
  scriptsMenu(id: string): void
  copyText(text: string): Promise<void>
  refresh(): void
  setVisible(visible: boolean): void
  quit(): void
}

declare global {
  interface Window {
    localhost: LocalhostApi
  }
}
