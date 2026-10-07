import { app } from 'electron'
import fs from 'node:fs'
import path from 'node:path'

export const isMac = process.platform === 'darwin'
export const isWin = process.platform === 'win32'

export function dataDir(...parts: string[]): string {
  return path.join(app.getPath('userData'), ...parts)
}

let logFile = ''

export function log(...args: unknown[]): void {
  const line = `${new Date().toISOString()} ${args.map((a) => (a instanceof Error ? a.stack || a.message : typeof a === 'string' ? a : JSON.stringify(a))).join(' ')}\n`
  if (!app.isPackaged) process.stdout.write(line)
  try {
    if (!logFile) {
      const dir = dataDir('logs')
      fs.mkdirSync(dir, { recursive: true })
      logFile = path.join(dir, 'localhost.log')
      try {
        if (fs.statSync(logFile).size > 2_000_000) fs.renameSync(logFile, path.join(dir, 'localhost.old.log'))
      } catch {}
    }
    fs.appendFileSync(logFile, line)
  } catch {}
}

export const str = (v: unknown, max = 4096): string => (typeof v === 'string' ? v.slice(0, max) : '')
