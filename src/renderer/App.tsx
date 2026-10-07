import { AnimatePresence, motion, MotionConfig } from 'motion/react'
import { useEffect, useRef } from 'react'
import type { Snapshot } from '../shared/types'
import { handleMenu, openService, stop } from './actions'
import { errorText } from './shared/format'
import { useSettings, useSnapshot, useTheme } from './shared/hooks'
import { tr } from './shared/i18n'
import { store, useStore, type Page } from './store'
import { DialogLayer, SearchField, ToastLayer } from './ui'
import { Detail } from './views/Detail'
import { Ports } from './views/Ports'
import { AllServices, OtherPage, ProjectActions, ProjectPage } from './views/Services'
import { Settings } from './views/Settings'
import { Sidebar } from './views/Sidebar'

const api = window.localhost

function pageTitle(page: Page, snapshot: Snapshot | null): { title: string; sub?: string } {
  switch (page.kind) {
    case 'all':
      return { title: tr('All services') }
    case 'ports':
      return { title: tr('Ports') }
    case 'other':
      return { title: tr('Other services') }
    case 'settings':
      return { title: tr('Settings') }
    case 'project': {
      const p = snapshot?.projects.find((x) => x.id === page.id)
      return { title: p?.name ?? tr('Project'), sub: p?.path }
    }
  }
}

export function App() {
  useTheme()
  const settings = useSettings()
  const snapshot = useSnapshot()
  const { page, detail, query, setQuery, open, close, flash, confirm } = useStore()
  const snapRef = useRef(snapshot)
  snapRef.current = snapshot

  useEffect(() => {
    const offMenu = api.onMenu((m) => handleMenu(snapRef.current, m.action, m.target, m.extra))
    const offEvent = api.onEvent((e) => {
      if (e.type === 'crashed') flash(tr('{0} stopped unexpectedly', e.title), { label: tr('Show'), run: () => open(e.id, 'logs') })
      else if (e.type === 'portInUse')
        flash(tr('Port {0} is in use, so {1} could not start', e.port, e.title), { label: tr('Show'), run: () => open(e.id, 'overview') })
      else
        confirm({
          title: tr('{0} is still running', e.title),
          body: tr("It didn't respond to the request to stop. You can force quit it."),
          confirm: tr('Force quit'),
          danger: true,
          onConfirm: async () => {
            const r = await api.stop(e.id, true)
            if (!r.ok) flash(errorText(r.error))
          },
        })
    })
    const offQuit = api.onQuitRequest((n) =>
      confirm({
        title: tr('Quit Localhost?'),
        body: tr('{0} service started by Localhost will stop.|{0} services started by Localhost will stop.', n),
        confirm: tr('Quit'),
        onConfirm: () => api.quit(),
      }),
    )
    const visibility = (): void => api.setVisible(document.visibilityState === 'visible')
    document.addEventListener('visibilitychange', visibility)
    return () => {
      offMenu()
      offEvent()
      offQuit()
      document.removeEventListener('visibilitychange', visibility)
    }
  }, [flash, open, confirm])

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (store().dialog) return
      const t = e.target as HTMLElement
      const typing = t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA'
      const mod = e.metaKey || e.ctrlKey
      if (mod && api.platform !== 'darwin') {
        const action = ({ f: 'find', r: 'refresh', ',': 'settings', o: 'add-project' } as Record<string, string>)[e.key.toLowerCase()]
        if (action) {
          e.preventDefault()
          handleMenu(snapRef.current, action, null, null)
          return
        }
      }
      if (typing) return
      const s = snapRef.current?.services.find((x) => x.id === store().detail)
      if (e.key === 'Escape' && store().detail) close()
      else if (e.key === '/' && !mod) {
        e.preventDefault()
        document.getElementById('search')?.focus()
      } else if (s && e.key === 'Enter' && mod) openService(s)
      else if (s && (e.key === 'Backspace' || e.key === 'Delete') && mod && s.controllable) stop(s)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [close])

  const head = pageTitle(page, snapshot)
  const project = page.kind === 'project' ? snapshot?.projects.find((p) => p.id === page.id) : undefined
  const reduced = settings?.motion === 'reduced'
  const searchable = page.kind !== 'settings'
  const key = detail ? `detail:${detail}` : page.kind === 'project' ? `project:${page.id}` : page.kind

  return (
    <MotionConfig reducedMotion={reduced ? 'always' : 'user'}>
      <div className="app">
        <Sidebar snapshot={snapshot} settings={settings} />
        <main className="content">
          <div className="drag" />
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={key}
              style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4, transition: { duration: 0.08 } }}
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            >
              {detail ? (
                <Detail id={detail} snapshot={snapshot} back={head.title} />
              ) : (
                <>
                  <div className="content-head">
                    <div className="content-heading">
                      <h1 className="content-title">{head.title}</h1>
                      {head.sub && <div className="content-sub">{head.sub}</div>}
                    </div>
                    <div className="content-actions">
                      {project && <ProjectActions project={project} />}
                      {searchable && <SearchField id="search" value={query} onChange={setQuery} placeholder={tr('Search services and ports')} />}
                    </div>
                  </div>
                  <div className="content-body">
                    {page.kind === 'all' && <AllServices snapshot={snapshot} settings={settings} query={query} />}
                    {page.kind === 'ports' && <Ports snapshot={snapshot} settings={settings} query={query} />}
                    {page.kind === 'other' && <OtherPage snapshot={snapshot} settings={settings} query={query} />}
                    {page.kind === 'project' && <ProjectPage snapshot={snapshot} settings={settings} query={query} id={page.id} />}
                    {page.kind === 'settings' && <Settings s={settings} />}
                  </div>
                </>
              )}
            </motion.div>
          </AnimatePresence>
        </main>
        <DialogLayer />
        <ToastLayer />
      </div>
    </MotionConfig>
  )
}
