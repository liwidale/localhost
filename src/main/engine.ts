import { app } from 'electron'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { EventEmitter } from 'node:events'
import path from 'node:path'
import { createInterface } from 'node:readline'
import type { EngineEvent, LogLine, Result, Settings, Snapshot } from '../shared/types'
import { dataDir, isWin, log } from './util'

type Pending = (r: Result<unknown>) => void

class EngineBridge extends EventEmitter {
  private child: ChildProcessWithoutNullStreams | null = null
  private pending = new Map<number, Pending>()
  private next = 0
  private stopping = false
  latest: Snapshot | null = null
  settings: Settings | null = null

  private binary(): string {
    const exe = isWin ? 'localhost-engine.exe' : 'localhost-engine'
    return app.isPackaged ? path.join(process.resourcesPath, 'engine', exe) : path.join(app.getAppPath(), 'resources', 'engine', exe)
  }

  start(): void {
    this.stopping = false
    const child = spawn(this.binary(), ['--data', dataDir(), '--host-pid', String(process.pid)], { stdio: 'pipe', windowsHide: true })
    this.child = child
    createInterface({ input: child.stdout }).on('line', (line) => this.receive(line))
    createInterface({ input: child.stderr }).on('line', (line) => log('engine:', line))
    child.on('error', (e) => log('engine failed to start', e))
    child.on('exit', (code) => {
      this.child = null
      for (const resolve of this.pending.values()) resolve({ ok: false, error: { code: 'failed', message: 'engine stopped' } })
      this.pending.clear()
      if (this.stopping) return
      log('engine exited', code)
      setTimeout(() => this.start(), 1000)
    })
    void this.call<Settings>('settings').then((r) => {
      if (r.ok) this.setSettings(r.value)
    })
  }

  private setSettings(s: Settings): void {
    this.settings = s
    this.emit('settings', s)
  }

  private receive(line: string): void {
    let msg: { id?: number; ok?: boolean; value?: unknown; error?: unknown; event?: string; data?: unknown }
    try {
      msg = JSON.parse(line)
    } catch {
      return
    }
    if (typeof msg.id === 'number') {
      const resolve = this.pending.get(msg.id)
      this.pending.delete(msg.id)
      resolve?.(msg.ok ? { ok: true, value: msg.value } : ({ ok: false, error: msg.error } as Result<unknown>))
      return
    }
    if (msg.event === 'snapshot') {
      this.latest = msg.data as Snapshot
      this.emit('snapshot', this.latest)
    } else if (msg.event === 'engine') {
      this.emit('engine', msg.data as EngineEvent)
    } else if (msg.event === 'logs') {
      this.emit('logs', msg.data as { sub: number; lines: LogLine[] })
    }
  }

  call<T>(method: string, params: unknown = null): Promise<Result<T>> {
    const child = this.child
    if (!child) return Promise.resolve({ ok: false, error: { code: 'failed', message: 'engine not running' } })
    const id = ++this.next
    return new Promise((resolve) => {
      this.pending.set(id, resolve as Pending)
      child.stdin.write(`${JSON.stringify({ id, method, params })}\n`)
    })
  }

  async updateSettings(patch: Partial<Settings>): Promise<Settings | null> {
    if (!this.settings) return null
    const r = await this.call<Settings>('setSettings', { ...this.settings, ...patch })
    if (r.ok) this.setSettings(r.value)
    return this.settings
  }

  stop(): Promise<void> {
    this.stopping = true
    const child = this.child
    if (!child) return Promise.resolve()
    return new Promise((resolve) => {
      const done = setTimeout(() => {
        child.kill()
        resolve()
      }, 4000)
      child.once('exit', () => {
        clearTimeout(done)
        resolve()
      })
      child.stdin.end()
    })
  }
}

export const engine = new EngineBridge()
