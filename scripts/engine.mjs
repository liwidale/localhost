import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const exe = process.platform === 'win32' ? 'localhost-engine.exe' : 'localhost-engine'
const debug = process.argv.includes('--debug')
const args = ['build', '-p', 'localhost-engine', ...(debug ? [] : ['--release'])]
const out = path.join(root, 'resources/engine')
mkdirSync(out, { recursive: true })

if (process.platform === 'darwin' && process.argv.includes('--universal')) {
  const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin']
  execFileSync('rustup', ['target', 'add', ...targets], { stdio: 'inherit', cwd: root })
  for (const t of targets) execFileSync('cargo', [...args, '--target', t], { stdio: 'inherit', cwd: root })
  execFileSync('lipo', ['-create', ...targets.map((t) => path.join(root, 'target', t, 'release', exe)), '-output', path.join(out, exe)])
} else {
  execFileSync('cargo', args, { stdio: 'inherit', cwd: root })
  copyFileSync(path.join(root, 'target', debug ? 'debug' : 'release', exe), path.join(out, exe))
}
console.log('engine staged')
