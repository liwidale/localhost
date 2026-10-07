import { BrowserWindow, nativeTheme } from 'electron'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { engine } from './engine'
import { isMac, isWin } from './util'

let win: BrowserWindow | null = null
let quitting = false

export const mainWindow = (): BrowserWindow | null => (win && !win.isDestroyed() ? win : null)

export const allowQuit = (): void => {
  quitting = true
}

const dark = (): boolean => nativeTheme.shouldUseDarkColors
const background = (): string => (dark() ? '#000000' : '#f3f3f3')
const overlay = () => ({ color: '#00000000', symbolColor: dark() ? '#f2f2f2' : '#1c1c1e', height: 44 })

export function refreshChrome(): void {
  const w = mainWindow()
  if (!w || !isWin) return
  try {
    w.setBackgroundColor(background())
    w.setTitleBarOverlay(overlay())
  } catch {}
}

function rendererUrl(): string {
  return pathToFileURL(path.join(__dirname, '../renderer/index.html')).toString()
}

export function createWindow(): BrowserWindow {
  const existing = mainWindow()
  if (existing) {
    if (existing.isMinimized()) existing.restore()
    existing.show()
    existing.focus()
    return existing
  }
  win = new BrowserWindow({
    width: 1180,
    height: 760,
    minWidth: 760,
    minHeight: 500,
    show: false,
    title: 'Localhost',
    backgroundColor: isMac ? '#00000000' : background(),
    autoHideMenuBar: true,
    ...(isMac
      ? {
          titleBarStyle: 'hiddenInset' as const,
          vibrancy: 'sidebar' as const,
          visualEffectState: 'followWindow' as const,
          transparent: true,
          trafficLightPosition: { x: 18, y: 18 },
        }
      : isWin
        ? { titleBarStyle: 'hidden' as const, titleBarOverlay: overlay() }
        : {}),
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  })
  const w = win
  if (!isMac) w.setMenu(null)
  w.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  w.webContents.on('will-navigate', (e) => e.preventDefault())
  void w.loadURL(rendererUrl())
  w.once('ready-to-show', () => {
    w.show()
    w.focus()
  })
  w.on('close', async (e) => {
    if (quitting) return
    e.preventDefault()
    const r = await engine.call<number>('runningManaged')
    const running = r.ok ? r.value : 0
    if (running > 0) w.webContents.send('quit:request', running)
    else {
      quitting = true
      w.close()
    }
  })
  const visible = (v: boolean) => () => void engine.call('setVisible', { visible: v })
  w.on('show', visible(true))
  w.on('restore', visible(true))
  w.on('hide', visible(false))
  w.on('minimize', visible(false))
  w.on('closed', () => {
    win = null
  })
  return w
}
