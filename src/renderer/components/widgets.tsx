// Side-column widgets. Each is small and calm (animation only on change). Which ones show, and their order, come from
// settings.widgets (Settings → Widgets).
import { useEffect, useRef, useState } from 'react'
import { START_MY_DAY_ID, type PcStats, type RoutineStatus, type WidgetName } from '@shared/types'
import { useUi } from '../state'

export const WIDGETS: Record<Exclude<WidgetName, 'clock'>, { label: string; blurb: string }> = {
  routine: { label: "Today's routine", blurb: 'Start my day steps and their progress' },
  recent: { label: 'Recent', blurb: 'Tiles you opened lately' },
  pc: { label: 'This PC', blurb: 'CPU and memory' },
  focus: { label: 'Focus timer', blurb: '25-minute focus blocks' },
  notes: { label: 'Quick notes', blurb: 'A scratchpad saved on this PC' },
}

export const WIDGET_NAMES = Object.keys(WIDGETS) as (keyof typeof WIDGETS)[]

/** Enabled widgets in their saved order, then the disabled ones (for the Settings list). */
export function orderedWidgets(enabled: WidgetName[]): (keyof typeof WIDGETS)[] {
  const on = enabled.filter((w): w is keyof typeof WIDGETS => w in WIDGETS)
  return [...on, ...WIDGET_NAMES.filter((w) => !on.includes(w))]
}

const stepClass: Record<RoutineStatus, string> = { running: 'run', done: 'done', failed: 'failed', finished: 'done' }

function openedText(iso: string): string {
  const d = new Date(iso)
  const now = new Date()
  if (d.toDateString() === now.toDateString()) return `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, '0')}`
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}

function RoutineWidget() {
  const { profile, routine, setDialog } = useUi()
  const tiles = profile?.tiles ?? []
  const steps = profile?.routines.find((r) => r.id === START_MY_DAY_ID)?.steps ?? []
  return (
    <section className="w">
      <h3>TODAY'S ROUTINE <button className="w-link" onClick={() => setDialog({ kind: 'workspaces' })}>Edit</button></h3>
      {steps.length === 0 ? <p className="empty">Nothing yet</p> : (
        <ol className="steps">
          {steps.map((s, i) => {
            const st = routine.steps[i]
            const name = tiles.find((t) => t.id === s.tileId)?.name ?? (s.hint ? appName(s.hint.exe) : 'Missing item')
            return (
              <li key={`${s.tileId ?? s.hint?.exe}-${i}`} className={`step${st ? ` ${stepClass[st]}` : ''}`}>
                <i>{st === 'failed' ? '!' : st === 'done' ? '✓' : i + 1}</i>
                <span>{name}</span>
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}

/** "ms-teams.exe" → "Ms Teams". */
export function appName(exe: string): string {
  return exe.replace(/\.exe$/i, '').split(/[-_. ]+/).filter(Boolean).map((w) => w[0].toUpperCase() + w.slice(1)).join(' ')
}

function RecentWidget() {
  const { profile, openTile } = useUi()
  const recent = (profile?.tiles ?? [])
    .filter((t) => t.lastOpenedAt)
    .sort((a, b) => (b.lastOpenedAt! < a.lastOpenedAt! ? -1 : b.lastOpenedAt! > a.lastOpenedAt! ? 1 : 0))
    .slice(0, 5)
  return (
    <section className="w">
      <h3>RECENT</h3>
      {recent.length === 0 ? <p className="empty">Tiles you open show up here</p> : (
        <ul className="recent">
          {recent.map((t) => (
            <li key={t.id}>
              <button onClick={() => void openTile(t)} title={`Open ${t.name}`}>
                <span>{t.name}</span>
                <small>{openedText(t.lastOpenedAt!)}</small>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function usePcStats(): PcStats | null {
  const [stats, setStats] = useState<PcStats | null>(null)
  useEffect(() => {
    let timer = 0
    let alive = true
    const poll = async () => {
      try {
        const s = await window.bridge.getPcStats()
        if (alive) setStats(s)
      } catch { /* keep the last value */ }
    }
    const start = () => { if (!timer) { void poll(); timer = window.setInterval(poll, 3000) } }
    const stop = () => { clearInterval(timer); timer = 0 }
    const onVis = () => (document.hidden ? stop() : start())
    if (!document.hidden) start()
    document.addEventListener('visibilitychange', onVis)
    return () => { alive = false; stop(); document.removeEventListener('visibilitychange', onVis) }
  }, [])
  return stats
}

function Gauge({ label, value }: { label: string; value: number | undefined }) {
  const v = value === undefined ? 0 : Math.max(0, Math.min(100, Math.round(value)))
  return (
    <div className={`gauge${v >= 85 ? ' hot' : ''}`}>
      {label}
      <b>{value === undefined ? '–' : `${v}%`}</b>
      <span className="track" style={{ '--v': `${v}%` } as React.CSSProperties} />
    </div>
  )
}

function PcWidget() {
  const stats = usePcStats()
  return (
    <section className="w">
      <h3>THIS PC</h3>
      <Gauge label="CPU" value={stats?.cpu} />
      <Gauge label="Memory" value={stats?.mem} />
    </section>
  )
}

const FOCUS_KEY = 'bridge.focus.endsAt'
const FOCUS_MIN = 25

function FocusWidget() {
  const { toast, reactor } = useUi()
  const [endsAt, setEndsAt] = useState<number | null>(() => Number(localStorage.getItem(FOCUS_KEY)) || null)
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    if (!endsAt) return
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [endsAt])
  useEffect(() => {
    if (endsAt && now >= endsAt) {
      setEndsAt(null)
      localStorage.removeItem(FOCUS_KEY)
      toast('Focus block done. Take a short break.', 'ok')
      reactor.current?.burst()
    }
  }, [now, endsAt, toast, reactor])
  const start = () => { const t = Date.now() + FOCUS_MIN * 60_000; localStorage.setItem(FOCUS_KEY, String(t)); setNow(Date.now()); setEndsAt(t) }
  const stop = () => { localStorage.removeItem(FOCUS_KEY); setEndsAt(null) }
  const left = endsAt ? Math.max(0, endsAt - now) : FOCUS_MIN * 60_000
  const mm = Math.floor(left / 60_000), ss = Math.floor((left % 60_000) / 1000)
  return (
    <section className="w">
      <h3>FOCUS</h3>
      <div className="focus">
        <b className={endsAt ? 'live' : ''}>{mm}:{String(ss).padStart(2, '0')}</b>
        <button className="edit" onClick={endsAt ? stop : start}>{endsAt ? 'Stop' : 'Start'}</button>
      </div>
      {endsAt && <span className="track focus-track" style={{ '--v': `${100 - (left / (FOCUS_MIN * 600))}%` } as React.CSSProperties} />}
    </section>
  )
}

const NOTES_KEY = 'bridge.notes'

function NotesWidget() {
  const [text, setText] = useState(() => localStorage.getItem(NOTES_KEY) ?? '')
  const timer = useRef(0)
  const change = (v: string) => {
    setText(v)
    clearTimeout(timer.current)
    timer.current = window.setTimeout(() => localStorage.setItem(NOTES_KEY, v), 300)
  }
  return (
    <section className="w">
      <h3>NOTES</h3>
      <textarea className="field notes" value={text} onChange={(e) => change(e.target.value)} placeholder="Jot something down…" rows={5} />
    </section>
  )
}

const RENDER: Record<keyof typeof WIDGETS, () => React.JSX.Element> = {
  routine: RoutineWidget,
  recent: RecentWidget,
  pc: PcWidget,
  focus: FocusWidget,
  notes: NotesWidget,
}

export function Widget({ name }: { name: WidgetName }) {
  const C = (RENDER as Record<string, (() => React.JSX.Element) | undefined>)[name]
  return C ? <C /> : null
}
