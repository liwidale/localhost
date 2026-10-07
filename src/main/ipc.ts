import { app, BrowserWindow, clipboard, dialog, ipcMain, nativeTheme, shell } from 'electron'
import type { Result, Settings, Target } from '../shared/types'
import { engine } from './engine'
import { tr } from './i18n'
import { projectMenu, scriptsMenu, serviceMenu } from './menus'
import { allowQuit, mainWindow } from './window'
import { str } from './util'

const target = (v: unknown): Target => (v === 'project' ? 'project' : 'service')

export const resolvedTheme = (): 'light' | 'dark' => (nativeTheme.shouldUseDarkColors ? 'dark' : 'light')

export function broadcast(channel: string, payload: unknown): void {
  for (const w of BrowserWindow.getAllWindows()) if (!w.isDestroyed()) w.webContents.send(channel, payload)
}

const failed = (message: string): Result<never> => ({ ok: false, error: { code: 'failed', message } })

export function registerIpc(): void {
  ipcMain.handle('snapshot', () => engine.latest)
  ipcMain.handle('settings:get', () => engine.settings)
  ipcMain.on('settings:language', (e) => {
    e.returnValue = engine.settings?.language ?? 'auto'
  })
  ipcMain.handle('settings:set', (_e, patch: Partial<Settings>) => engine.updateSettings(patch && typeof patch === 'object' ? patch : {}))
  ipcMain.handle('theme', () => resolvedTheme())
  ipcMain.on('app:version', (e) => {
    e.returnValue = app.getVersion()
  })
  ipcMain.handle('editor', async () => {
    const r = await engine.call<string | null>('editor')
    return r.ok ? r.value : null
  })

  ipcMain.handle('inspect', (_e, id) => engine.call('inspect', { id: str(id) }))
  ipcMain.handle('reveal', (_e, id, name) => engine.call('reveal', { id: str(id), name: str(name, 512) }))
  ipcMain.handle('stop', (_e, id, force) => engine.call('stop', { id: str(id), force: force === true }))
  ipcMain.handle('restart', (_e, id) => engine.call('restart', { id: str(id) }))
  ipcMain.handle('stopProject', (_e, projectId) => engine.call('stopProject', { projectId: str(projectId) }))
  ipcMain.handle('startScript', (_e, projectId, script, trust) =>
    engine.call('startScript', { projectId: str(projectId), script: str(script), trust: trust === true }),
  )
  ipcMain.handle('dismiss', (_e, id) => engine.call('dismiss', { id: str(id) }))
  ipcMain.handle('clearLogs', (_e, id) => engine.call('clearLogs', { id: str(id) }))
  ipcMain.handle('logs:subscribe', (_e, id) => engine.call('subscribeLogs', { id: str(id) }))
  ipcMain.handle('logs:unsubscribe', (_e, subscription) => engine.call('unsubscribeLogs', { subscription: Number(subscription) || 0 }))

  ipcMain.handle('open:url', async (_e, id, port) => {
    const r = await engine.call<string>('url', { id: str(id), port: Number(port) || 0 })
    if (!r.ok) return r
    if (!/^https?:\/\/[^/\s]+$/i.test(r.value)) return failed('invalid address')
    await shell.openExternal(r.value)
    return { ok: true, value: null }
  })
  ipcMain.handle('open:folder', async (_e, kind, id) => {
    const r = await engine.call<string>('folder', { target: target(kind), id: str(id) })
    if (!r.ok) return r
    const error = await shell.openPath(r.value)
    return error ? failed(error) : { ok: true, value: null }
  })
  ipcMain.handle('open:terminal', (_e, kind, id) => engine.call('openTerminal', { target: target(kind), id: str(id) }))
  ipcMain.handle('open:editor', (_e, kind, id) => engine.call('openEditor', { target: target(kind), id: str(id) }))

  ipcMain.handle('project:add', async (e) => {
    const parent = BrowserWindow.fromWebContents(e.sender)
    const options = { title: tr('Choose a project folder'), properties: ['openDirectory' as const] }
    const picked = await (parent ? dialog.showOpenDialog(parent, options) : dialog.showOpenDialog(options))
    if (picked.canceled || !picked.filePaths[0]) return { ok: true, value: null }
    return engine.call<string>('addProject', { path: picked.filePaths[0] })
  })
  ipcMain.handle('project:keep', (_e, projectId) => engine.call('keepProject', { projectId: str(projectId) }))
  ipcMain.handle('project:remove', (_e, projectId) => engine.call('removeProject', { projectId: str(projectId) }))

  ipcMain.on('menu:service', (e, id) => serviceMenu(BrowserWindow.fromWebContents(e.sender), str(id)))
  ipcMain.on('menu:project', (e, id) => projectMenu(BrowserWindow.fromWebContents(e.sender), str(id)))
  ipcMain.on('menu:scripts', (e, id) => scriptsMenu(BrowserWindow.fromWebContents(e.sender), str(id)))

  ipcMain.handle('clipboard:write', (_e, text) => clipboard.writeText(str(text, 1_000_000)))
  ipcMain.on('app:refresh', () => void engine.call('refresh'))
  ipcMain.on('app:visible', (_e, visible) => void engine.call('setVisible', { visible: visible === true }))
  ipcMain.on('app:quit', () => {
    allowQuit()
    mainWindow()?.close()
  })
}
