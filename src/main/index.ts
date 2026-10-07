import { app, nativeTheme, Notification } from 'electron'
import type { EngineEvent, Settings } from '../shared/types'
import { engine } from './engine'
import { tr } from './i18n'
import { broadcast, registerIpc, resolvedTheme } from './ipc'
import { appMenu, loadEditor } from './menus'
import { allowQuit, createWindow, mainWindow, refreshChrome } from './window'
import { isMac, log } from './util'

if (!app.requestSingleInstanceLock()) {
  app.quit()
  process.exit(0)
}

app.setName('Localhost')
if (process.platform === 'win32') app.setAppUserModelId('com.liwidale.localhost')

let language = ''

function applySettings(s: Settings): void {
  nativeTheme.themeSource = s.theme
  if (language && language !== s.language) mainWindow()?.webContents.reload()
  language = s.language
  if (isMac) appMenu()
  broadcast('settings', s)
}

function notify(event: EngineEvent): void {
  if (!engine.settings?.notifications || mainWindow()?.isFocused() || !Notification.isSupported()) return
  if (event.type === 'crashed') {
    new Notification({
      title: tr('{0} stopped', event.title),
      body:
        event.code !== null
          ? tr('It exited with code {0}. Open Localhost to see its logs.', event.code)
          : tr('It exited unexpectedly. Open Localhost to see its logs.'),
    }).show()
  } else if (event.type === 'portInUse') {
    new Notification({
      title: tr('Port {0} is already in use', event.port),
      body: tr('{0} could not start because another process is using the port.', event.title),
    }).show()
  }
}

app.on('second-instance', () => createWindow())

app.whenReady().then(async () => {
  registerIpc()
  engine.on('snapshot', (s) => broadcast('snapshot', s))
  engine.on('logs', (l) => broadcast('logs', l))
  engine.on('engine', (e: EngineEvent) => {
    notify(e)
    broadcast('engine', e)
  })
  engine.on('settings', applySettings)
  nativeTheme.on('updated', () => {
    refreshChrome()
    broadcast('theme', resolvedTheme())
  })
  const ready = new Promise<void>((resolve) => {
    engine.once('settings', () => resolve())
    setTimeout(resolve, 2000)
  })
  engine.start()
  void loadEditor()
  await ready
  if (isMac) appMenu()
  createWindow()
})

app.on('activate', () => createWindow())

app.on('before-quit', (e) => {
  const w = mainWindow()
  if (w) {
    e.preventDefault()
    w.close()
  }
})

app.on('window-all-closed', () => {
  allowQuit()
  void engine.stop().then(() => {
    log('quit')
    app.exit(0)
  })
})
