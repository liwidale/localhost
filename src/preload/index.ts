import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { LocalhostApi } from '../shared/api'
import type { LogLine, Result } from '../shared/types'

const on =
  <T>(channel: string) =>
  (cb: (v: T) => void): (() => void) => {
    const fn = (_e: IpcRendererEvent, v: T): void => cb(v)
    ipcRenderer.on(channel, fn)
    return () => ipcRenderer.removeListener(channel, fn)
  }

const api: LocalhostApi = {
  platform: process.platform as LocalhostApi['platform'],
  language: String(ipcRenderer.sendSync('settings:language') || 'auto'),
  version: String(ipcRenderer.sendSync('app:version') || ''),
  snapshot: () => ipcRenderer.invoke('snapshot'),
  onSnapshot: on('snapshot'),
  onEvent: on('engine'),
  onMenu: on('menu'),
  onQuitRequest: on('quit:request'),
  settings: () => ipcRenderer.invoke('settings:get'),
  setSettings: (patch) => ipcRenderer.invoke('settings:set', patch),
  onSettings: on('settings'),
  theme: () => ipcRenderer.invoke('theme'),
  onTheme: on('theme'),
  editor: () => ipcRenderer.invoke('editor'),

  inspect: (id) => ipcRenderer.invoke('inspect', id),
  reveal: (id, name) => ipcRenderer.invoke('reveal', id, name),
  stop: (id, force) => ipcRenderer.invoke('stop', id, force),
  restart: (id) => ipcRenderer.invoke('restart', id),
  stopProject: (projectId) => ipcRenderer.invoke('stopProject', projectId),
  startScript: (projectId, script, trust) => ipcRenderer.invoke('startScript', projectId, script, trust),
  dismiss: (id) => ipcRenderer.invoke('dismiss', id),
  clearLogs: (id) => ipcRenderer.invoke('clearLogs', id),
  subscribeLogs: async (id, cb) => {
    const early: { sub: number; lines: LogLine[] }[] = []
    let sub = -1
    const fn = (_e: IpcRendererEvent, data: { sub: number; lines: LogLine[] }): void => {
      if (sub === -1) early.push(data)
      else if (data.sub === sub) cb(data.lines)
    }
    ipcRenderer.on('logs', fn)
    const r: Result<{ sub: number; backlog: LogLine[] }> = await ipcRenderer.invoke('logs:subscribe', id)
    if (!r.ok) {
      ipcRenderer.removeListener('logs', fn)
      return () => {}
    }
    sub = r.value.sub
    cb(r.value.backlog)
    for (const e of early) if (e.sub === sub) cb(e.lines)
    return () => {
      ipcRenderer.removeListener('logs', fn)
      void ipcRenderer.invoke('logs:unsubscribe', sub)
    }
  },
  open: (id, port) => ipcRenderer.invoke('open:url', id, port),
  openFolder: (target, id) => ipcRenderer.invoke('open:folder', target, id),
  openTerminal: (target, id) => ipcRenderer.invoke('open:terminal', target, id),
  openEditor: (target, id) => ipcRenderer.invoke('open:editor', target, id),
  addProject: () => ipcRenderer.invoke('project:add'),
  keepProject: (projectId) => ipcRenderer.invoke('project:keep', projectId),
  removeProject: (projectId) => ipcRenderer.invoke('project:remove', projectId),
  serviceMenu: (id) => ipcRenderer.send('menu:service', id),
  projectMenu: (id) => ipcRenderer.send('menu:project', id),
  scriptsMenu: (id) => ipcRenderer.send('menu:scripts', id),
  copyText: (text) => ipcRenderer.invoke('clipboard:write', text),
  refresh: () => ipcRenderer.send('app:refresh'),
  setVisible: (visible) => ipcRenderer.send('app:visible', visible),
  quit: () => ipcRenderer.send('app:quit'),
}

contextBridge.exposeInMainWorld('localhost', api)
