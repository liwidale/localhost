import { build as esbuild } from 'esbuild'
import { build as vite } from 'vite'
import react from '@vitejs/plugin-react'
import { rmSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const dev = process.argv.includes('--dev')

rmSync(path.join(root, 'out'), { recursive: true, force: true })

const common = {
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'cjs',
  sourcemap: dev ? 'inline' : false,
  minify: !dev,
  external: ['electron'],
  logLevel: 'warning',
}

await Promise.all([
  esbuild({ ...common, entryPoints: [path.join(root, 'src/main/index.ts')], outfile: path.join(root, 'out/main/index.js') }),
  esbuild({ ...common, entryPoints: [path.join(root, 'src/preload/index.ts')], outfile: path.join(root, 'out/preload/index.js') }),
])

await vite({
  configFile: false,
  root: path.join(root, 'src/renderer'),
  base: './',
  plugins: [react()],
  logLevel: 'warn',
  build: {
    outDir: path.join(root, 'out/renderer'),
    emptyOutDir: true,
    sourcemap: dev,
    minify: !dev,
    target: 'chrome130',
  },
})

console.log('built', dev ? '(dev)' : '')
