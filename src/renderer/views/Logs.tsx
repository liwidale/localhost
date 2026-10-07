import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import type { LogLine, Service, Span } from '../../shared/types'
import { copy, restart } from '../actions'
import { clock } from '../shared/format'
import { tr } from '../shared/i18n'
import { Btn, Empty, SearchField } from '../ui'

const ROW = 18
const LIMIT = 5000

function color(s: Span): CSSProperties | undefined {
  if (!s.fg) return undefined
  return { color: s.fg.startsWith('a') ? `var(--ansi-${s.fg.slice(1)})` : s.fg }
}

function highlight(text: string, q: string) {
  const lower = text.toLowerCase()
  const out: ReactNode[] = []
  let i = 0
  while (q && i < text.length) {
    const j = lower.indexOf(q, i)
    if (j < 0) break
    if (j > i) out.push(text.slice(i, j))
    out.push(<mark key={j}>{text.slice(j, j + q.length)}</mark>)
    i = j + q.length
  }
  if (i < text.length) out.push(text.slice(i))
  return out
}

function Line({ line, query, timestamps }: { line: LogLine; query: string; timestamps: boolean }) {
  return (
    <div className={`line ${line.level} ${line.stream}`}>
      {timestamps && <span className="ts">{clock(line.ts)}</span>}
      {query
        ? highlight(line.text, query)
        : line.spans
          ? line.spans.map((s, i) => (
              <span key={i} className={[s.b && 's-b', s.d && 's-d', s.i && 's-i', s.u && 's-u'].filter(Boolean).join(' ') || undefined} style={color(s)}>
                {s.t}
              </span>
            ))
          : line.text}
    </div>
  )
}

export function Logs({ s }: { s: Service }) {
  const [lines, setLines] = useState<LogLine[]>([])
  const [pending, setPending] = useState<LogLine[]>([])
  const [paused, setPaused] = useState(false)
  const [follow, setFollow] = useState(true)
  const [unseen, setUnseen] = useState(0)
  const [query, setQuery] = useState('')
  const [timestamps, setTimestamps] = useState(false)
  const [scrollTop, setScrollTop] = useState(0)
  const [height, setHeight] = useState(400)
  const scroller = useRef<HTMLDivElement>(null)
  const pausedRef = useRef(paused)
  const followRef = useRef(follow)
  pausedRef.current = paused
  followRef.current = follow

  useEffect(() => {
    if (!s.hasLogs) return
    let stop: (() => void) | null = null
    let alive = true
    void window.localhost
      .subscribeLogs(s.id, (batch) => {
        if (!batch.length) return
        if (pausedRef.current) return setPending((p) => [...p, ...batch].slice(-LIMIT))
        setLines((l) => (l.length + batch.length > LIMIT ? [...l, ...batch].slice(-LIMIT) : [...l, ...batch]))
        if (!followRef.current) setUnseen((n) => n + batch.length)
      })
      .then((fn) => {
        if (alive) stop = fn
        else fn()
      })
    return () => {
      alive = false
      stop?.()
    }
  }, [s.id, s.hasLogs])

  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const ro = new ResizeObserver(() => setHeight(el.clientHeight))
    ro.observe(el)
    return () => ro.disconnect()
  }, [s.hasLogs])

  const q = query.trim().toLowerCase()
  const filtered = useMemo(() => (q ? lines.filter((l) => l.text.toLowerCase().includes(q)) : lines), [lines, q])

  useLayoutEffect(() => {
    if (follow && scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight
  }, [filtered, follow])

  if (!s.hasLogs) {
    return (
      <div className="content-body">
        {s.controllable && s.projectId ? (
          <Empty
            icon={false}
            title={tr("Logs aren't available for this process")}
            body={tr(
              'It was started outside Localhost, so its output goes to the terminal that started it. Restart it in Localhost to see its output here from now on.',
            )}
            action={
              <Btn kind="primary" icon="refresh" onClick={() => restart(s)}>
                {tr('Restart in Localhost')}
              </Btn>
            }
          />
        ) : (
          <Empty
            icon={false}
            title={tr("Logs aren't available for this process")}
            body={tr("It was started outside Localhost, so its output isn't available here.")}
          />
        )}
      </div>
    )
  }

  const start = Math.max(0, Math.floor(scrollTop / ROW) - 20)
  const end = Math.min(filtered.length, start + Math.ceil(height / ROW) + 40)

  const onScroll = (): void => {
    const el = scroller.current
    if (!el) return
    setScrollTop(el.scrollTop)
    const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < ROW * 2
    setFollow(atEnd)
    if (atEnd) setUnseen(0)
  }
  const resume = (): void => {
    setPaused(false)
    setLines((l) => [...l, ...pending].slice(-LIMIT))
    setPending([])
  }
  const jump = (): void => {
    if (paused) resume()
    setFollow(true)
    setUnseen(0)
  }
  const clear = (): void => {
    setLines([])
    setPending([])
    setUnseen(0)
    if (s.source === 'managed') void window.localhost.clearLogs(s.id)
  }
  const waiting = paused ? pending.length : unseen

  return (
    <div className="logs">
      <div className="logs-bar">
        <SearchField value={query} onChange={setQuery} placeholder={tr('Filter')} />
        <Btn small kind="ghost" icon="clock" title={tr('Timestamps')} on={timestamps} onClick={() => setTimestamps(!timestamps)} />
        <Btn
          small
          kind="ghost"
          icon={paused ? 'play' : 'pause'}
          title={paused ? tr('Resume') : tr('Pause')}
          on={paused}
          onClick={() => (paused ? resume() : setPaused(true))}
        />
        <Btn
          small
          kind="ghost"
          icon="copy"
          title={tr('Copy all')}
          onClick={() =>
            void copy(
              filtered.map((l) => (timestamps ? `${clock(l.ts)}  ${l.text}` : l.text)).join('\n'),
              tr('Copied {0} line|Copied {0} lines', filtered.length),
            )
          }
        />
        <Btn small kind="ghost" icon="trash" title={tr('Clear')} onClick={clear} />
      </div>
      <div className="logs-scroll" ref={scroller} onScroll={onScroll} role="log">
        {filtered.length === 0 && <div className="logs-empty">{q ? tr('No lines match “{0}”', query) : tr('No output yet.')}</div>}
        <div className="logs-spacer" style={{ height: filtered.length * ROW }}>
          <div className="logs-rows" style={{ transform: `translateY(${start * ROW}px)` }}>
            {filtered.slice(start, end).map((line) => (
              <Line key={line.seq} line={line} query={q} timestamps={timestamps} />
            ))}
          </div>
        </div>
      </div>
      {waiting > 0 && (
        <div className="jump">
          <Btn kind="primary" small icon="down" onClick={jump}>
            {tr('{0} new line|{0} new lines', waiting)}
          </Btn>
        </div>
      )}
    </div>
  )
}
