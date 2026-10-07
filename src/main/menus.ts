import { BrowserWindow, Menu, type MenuItemConstructorOptions } from 'electron'
import type { MenuAction } from '../shared/types'
import { engine } from './engine'
import { tr } from './i18n'
import { mainWindow } from './window'

let editorName: string | null = null

export async function loadEditor(): Promise<void> {
  const r = await engine.call<string | null>('editor')
  editorName = r.ok ? r.value : null
}

function emit(action: string, target: string | null = null, extra: string | null = null): void {
  const payload: MenuAction = { action, target, extra }
  mainWindow()?.webContents.send('menu', payload)
}

const item = (label: string, action: string, target: string | null = null, extra: string | null = null): MenuItemConstructorOptions => ({
  label,
  click: () => emit(action, target, extra),
})

function popup(win: BrowserWindow | null, groups: MenuItemConstructorOptions[][]): void {
  const template = groups.filter((g) => g.length).flatMap((g, i) => (i ? [{ type: 'separator' as const }, ...g] : g))
  if (template.length && win) Menu.buildFromTemplate(template).popup({ window: win })
}

export function appMenu(): void {
  const template: MenuItemConstructorOptions[] = [
    {
      label: 'Localhost',
      submenu: [
        { role: 'about', label: tr('About Localhost') },
        { type: 'separator' },
        { ...item(tr('Settings…'), 'settings'), accelerator: 'CmdOrCtrl+,' },
        { type: 'separator' },
        { role: 'hide', label: tr('Hide Localhost') },
        { role: 'hideOthers', label: tr('Hide others') },
        { role: 'unhide', label: tr('Show all') },
        { type: 'separator' },
        { role: 'quit', label: tr('Quit Localhost') },
      ],
    },
    {
      label: tr('File'),
      submenu: [
        { ...item(tr('Add project…'), 'add-project'), accelerator: 'CmdOrCtrl+O' },
        { type: 'separator' },
        { role: 'close', label: tr('Close window') },
      ],
    },
    {
      label: tr('Edit'),
      submenu: [
        { role: 'undo', label: tr('Undo') },
        { role: 'redo', label: tr('Redo') },
        { type: 'separator' },
        { role: 'cut', label: tr('Cut') },
        { role: 'copy', label: tr('Copy') },
        { role: 'paste', label: tr('Paste') },
        { role: 'selectAll', label: tr('Select all') },
        { type: 'separator' },
        { ...item(tr('Find'), 'find'), accelerator: 'CmdOrCtrl+F' },
      ],
    },
    {
      label: tr('View'),
      submenu: [
        { ...item(tr('Refresh'), 'refresh'), accelerator: 'CmdOrCtrl+R' },
        { ...item(tr('Show system services'), 'toggle-system'), accelerator: 'CmdOrCtrl+Shift+S' },
        { type: 'separator' },
        { role: 'togglefullscreen', label: tr('Enter full screen') },
      ],
    },
    {
      label: tr('Window'),
      submenu: [
        { role: 'minimize', label: tr('Minimize') },
        { role: 'zoom', label: tr('Zoom') },
      ],
    },
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

export function serviceMenu(win: BrowserWindow | null, id: string): void {
  const s = engine.latest?.services.find((x) => x.id === id)
  if (!s) return
  const live = s.pid !== null || s.source === 'container'
  const open: MenuItemConstructorOptions[] = []
  if (s.ports.some((p) => p.url)) open.push(item(tr('Open in browser'), 'open', id))
  if (s.ports.length) open.push(item(tr('Copy address'), 'copy', id))
  if (s.hasLogs) open.push(item(tr('Show logs'), 'logs', id))
  const control: MenuItemConstructorOptions[] = []
  if (s.controllable) {
    if (s.source === 'managed' && !live) {
      control.push(item(tr('Start'), 'start', id), item(tr('Remove from list'), 'dismiss', id))
    } else {
      if (s.source !== 'process' || s.projectId) control.push(item(s.source === 'process' ? tr('Restart in Localhost') : tr('Restart'), 'restart', id))
      control.push(item(tr('Stop'), 'stop', id), item(tr('Force quit'), 'kill', id))
    }
  }
  const folder: MenuItemConstructorOptions[] = []
  if (s.source !== 'container' || s.projectId) {
    folder.push(item(tr('Show in folder'), 'folder', id), item(tr('Open in terminal'), 'terminal', id))
    if (editorName) folder.push(item(tr('Open in {0}', editorName), 'editor', id))
  }
  popup(win, [open, control, folder])
}

export function projectMenu(win: BrowserWindow | null, id: string): void {
  const snap = engine.latest
  const p = snap?.projects.find((x) => x.id === id)
  if (!p || !snap) return
  const running = snap.services.some((s) => s.projectId === id && s.controllable && s.pid !== null)
  const open = [item(tr('Show in folder'), 'folder', id), item(tr('Open in terminal'), 'terminal', id)]
  if (editorName) open.push(item(tr('Open in {0}', editorName), 'editor', id))
  popup(win, [
    open,
    running ? [item(tr('Stop all'), 'stop-all', id)] : [],
    [p.saved ? item(tr('Remove from Localhost'), 'remove', id) : item(tr('Keep in Localhost'), 'keep', id)],
  ])
}

export function scriptsMenu(win: BrowserWindow | null, id: string): void {
  const p = engine.latest?.projects.find((x) => x.id === id)
  if (!p) return
  const label = (s: { package: string; command: string }) => (s.package ? `${s.package}  ·  ${s.command}` : s.command)
  const main = p.scripts.filter((s) => s.longRunning).map((s) => item(label(s), 'run', id, s.id))
  const rest = p.scripts
    .filter((s) => !s.longRunning)
    .slice(0, 40)
    .map((s) => item(label(s), 'run', id, s.id))
  popup(win, [main, rest.length ? [{ label: tr('More scripts'), submenu: rest }] : []])
}
