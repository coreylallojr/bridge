// Start my day & workspaces: the layout editor. Your real monitors drawn to scale; each step opens a tile (or just
// moves an app that's already open) and puts its window in a zone. "Use my current windows" captures a workspace
// from what's on screen right now.
import { useEffect, useMemo, useState } from 'react'
import { START_MY_DAY_ID, ZONES, type DisplayInfo, type Routine, type RoutineStep, type Zone } from '@shared/types'
import { useUi } from '../state'
import { appName } from './widgets'

const T = 1 / 3
/** Zone → [x, y, w, h] as fractions of the screen (mirror of bridge-core zones.rs). */
const FRACTIONS: Record<Zone, [number, number, number, number]> = {
  full: [0, 0, 1, 1], left: [0, 0, 0.5, 1], right: [0.5, 0, 0.5, 1], top: [0, 0, 1, 0.5], bottom: [0, 0.5, 1, 0.5],
  'left-third': [0, 0, T, 1], 'center-third': [T, 0, T, 1], 'right-third': [2 * T, 0, T, 1],
  'left-two-thirds': [0, 0, 2 * T, 1], 'right-two-thirds': [T, 0, 2 * T, 1],
  'top-third': [0, 0, 1, T], 'middle-third': [0, T, 1, T], 'bottom-third': [0, 2 * T, 1, T],
  'top-left': [0, 0, 0.5, 0.5], 'top-right': [0.5, 0, 0.5, 0.5], 'bottom-left': [0, 0.5, 0.5, 0.5], 'bottom-right': [0.5, 0.5, 0.5, 0.5],
}

export const ZONE_LABEL: Record<Zone, string> = {
  full: 'Full screen', left: 'Left half', right: 'Right half', top: 'Top half', bottom: 'Bottom half',
  'left-third': 'Left third', 'center-third': 'Center third', 'right-third': 'Right third',
  'left-two-thirds': 'Left two-thirds', 'right-two-thirds': 'Right two-thirds',
  'top-third': 'Top third', 'middle-third': 'Middle third', 'bottom-third': 'Bottom third',
  'top-left': 'Top-left', 'top-right': 'Top-right', 'bottom-left': 'Bottom-left', 'bottom-right': 'Bottom-right',
}

const pct = (f: [number, number, number, number]) =>
  ({ left: `${f[0] * 100}%`, top: `${f[1] * 100}%`, width: `${f[2] * 100}%`, height: `${f[3] * 100}%` })

const MAP_W = 640
const MAP_H = 230

export function Workspaces() {
  const { dialog, setDialog, profile, toast, reactor } = useUi()
  const open = dialog.kind === 'workspaces'
  const routines = profile?.routines ?? []
  const tiles = profile?.tiles ?? []
  const [displays, setDisplays] = useState<DisplayInfo[]>([])
  const [draft, setDraft] = useState<Routine | null>(null)
  const [sel, setSel] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [addTile, setAddTile] = useState('')

  // Open on the requested routine (or Start my day); refresh the monitor list each time.
  useEffect(() => {
    if (!open) return
    window.bridge.listDisplays().then(setDisplays).catch(() => setDisplays([]))
    const want = (dialog.kind === 'workspaces' && dialog.routineId) || START_MY_DAY_ID
    const r = routines.find((x) => x.id === want) ?? routines[0]
    setDraft(r ? structuredClone(r) : null)
    setSel(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); setDialog({ kind: 'none' }) } }
    addEventListener('keydown', onKey, true)
    return () => removeEventListener('keydown', onKey, true)
  }, [open, setDialog])

  const saved = draft && routines.find((r) => r.id === draft.id)
  const dirty = !!draft && JSON.stringify(saved ?? null) !== JSON.stringify(draft)

  // Monitors to scale.
  const layout = useMemo(() => {
    const ds = displays.filter((d) => d.bounds && d.bounds.width > 0)
    if (!ds.length) return []
    const minX = Math.min(...ds.map((d) => d.bounds.x)), minY = Math.min(...ds.map((d) => d.bounds.y))
    const maxX = Math.max(...ds.map((d) => d.bounds.x + d.bounds.width)), maxY = Math.max(...ds.map((d) => d.bounds.y + d.bounds.height))
    const k = Math.min(MAP_W / (maxX - minX), MAP_H / (maxY - minY))
    return ds.map((d) => ({
      d,
      style: { left: (d.bounds.x - minX) * k, top: (d.bounds.y - minY) * k, width: d.bounds.width * k - 6, height: d.bounds.height * k - 6 },
    }))
  }, [displays])

  if (!open) return null

  const stepName = (s: RoutineStep) =>
    tiles.find((t) => t.id === s.tileId)?.name ?? (s.hint ? appName(s.hint.exe) : 'Missing item')
  const screenName = (id: number) => displays.find((d) => d.id === id)?.label.split(' · ')[0] ?? 'A screen that is unplugged'
  const setSteps = (steps: RoutineStep[]) => draft && setDraft({ ...draft, steps })
  const patchStep = (i: number, p: Partial<RoutineStep>) =>
    draft && setSteps(draft.steps.map((s, j) => (j === i ? { ...s, ...p } : s)))
  const move = (i: number, d: number) => {
    if (!draft) return
    const j = i + d
    if (j < 0 || j >= draft.steps.length) return
    const next = [...draft.steps]
    ;[next[i], next[j]] = [next[j], next[i]]
    setSteps(next)
    setSel(j)
  }

  const save = async (): Promise<Routine | null> => {
    if (!draft) return null
    const r = await window.bridge.saveRoutine(draft)
    if (!r.ok || !r.data) { toast(r.message, 'error'); return null }
    setDraft(structuredClone(r.data))
    return r.data
  }

  const run = async () => {
    setBusy(true)
    try {
      const r = dirty || !saved ? await save() : draft
      if (!r) return
      setDialog({ kind: 'none' })
      if (r.id === START_MY_DAY_ID) document.getElementById('start-my-day')?.click()
      else {
        reactor.current?.burst()
        const res = await window.bridge.runRoutine(r.id)
        toast(res.message, res.ok ? 'ok' : 'error')
      }
    } finally { setBusy(false) }
  }

  const capture = async () => {
    setBusy(true)
    try {
      const found = await window.bridge.captureLayout()
      if (!found.length) { toast('No app windows to capture. Open and arrange some first.', 'error'); return }
      const n = routines.filter((r) => r.id !== START_MY_DAY_ID).length + 1
      setDraft({ id: '', name: `Workspace ${n}`, steps: found.map((c) => c.step) })
      setSel(null)
      toast(`Captured ${found.length} window${found.length === 1 ? '' : 's'}. Check the list, then Save.`, 'ok')
    } finally { setBusy(false) }
  }

  const remove = async () => {
    if (!draft || !draft.id || draft.id === START_MY_DAY_ID) return
    const r = await window.bridge.deleteRoutine(draft.id)
    toast(r.message, r.ok ? 'ok' : 'error')
    if (r.ok) { const first = routines.find((x) => x.id === START_MY_DAY_ID); setDraft(first ? structuredClone(first) : null) }
  }

  const step = sel !== null ? draft?.steps[sel] : undefined

  return (
    <div className="scrim open" onMouseDown={(e) => { if (e.target === e.currentTarget) setDialog({ kind: 'none' }) }}>
      <section className="panel ws" role="dialog" aria-modal="true" aria-label="Start my day and workspaces">
        <header>
          <h2>Start my day &amp; workspaces</h2>
          <p>Open apps and put their windows where you want them</p>
          <button className="x" onClick={() => setDialog({ kind: 'none' })}>Close</button>
        </header>

        <div className="ws-grid">
          <nav className="ws-list" aria-label="Routines">
            {routines.map((r) => (
              <button key={r.id} className={draft?.id === r.id ? 'on' : ''}
                onClick={() => { setDraft(structuredClone(r)); setSel(null) }}>
                {r.name}<small>{r.steps.length} step{r.steps.length === 1 ? '' : 's'}</small>
              </button>
            ))}
            {draft && !draft.id && <button className="on">{draft.name}<small>not saved yet</small></button>}
            <button className="add" onClick={() => { setDraft({ id: '', name: `Workspace ${routines.length}`, steps: [] }); setSel(null) }}>+ New workspace</button>
            <button className="add" disabled={busy} onClick={() => void capture()}>+ Use my current windows</button>
          </nav>

          {draft && (
            <div className="ws-edit">
              <div className="row">
                <input className="field" aria-label="Name" value={draft.name} disabled={draft.id === START_MY_DAY_ID}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
              </div>

              <div className="ws-map" style={{ width: MAP_W, height: MAP_H }} aria-label="Your screens">
                {layout.map(({ d, style }) => {
                  const here = draft.steps.map((s, i) => ({ s, i })).filter(({ s }) => s.place?.displayId === d.id)
                  const pick = step && step.place?.displayId === d.id
                  return (
                    <button key={d.id} className={`mon${pick ? ' pick' : ''}${step ? ' armed' : ''}`} style={style}
                      title={step ? `Put ${stepName(step)} on ${d.label}` : d.label}
                      onClick={() => sel !== null && patchStep(sel, { place: { displayId: d.id, zone: step?.place?.zone ?? 'full' } })}>
                      <span className="mon-label">{d.label.split(' · ')[0]}{d.primary ? ' · main' : ''}</span>
                      {here.map(({ s, i }) => (
                        <span key={i} className={`zone${sel === i ? ' sel' : ''}`} style={pct(FRACTIONS[s.place!.zone])}>
                          <em>{i + 1}</em>{stepName(s)}
                        </span>
                      ))}
                    </button>
                  )
                })}
                {!layout.length && <p className="muted">Looking for your screens…</p>}
              </div>

              {step && sel !== null && (
                <div className="zone-pick">
                  <span className="muted small">
                    {step.place ? `${stepName(step)} → ${screenName(step.place.displayId)}, ${ZONE_LABEL[step.place.zone]}` : `Click a screen above to place ${stepName(step)}`}
                  </span>
                  {step.place && (
                    <div className="zones">
                      {ZONES.map((z) => (
                        <button key={z} className={`zbtn${step.place?.zone === z ? ' on' : ''}`} title={ZONE_LABEL[z]} aria-label={ZONE_LABEL[z]}
                          onClick={() => patchStep(sel, { place: { displayId: step.place!.displayId, zone: z } })}>
                          <span style={pct(FRACTIONS[z])} />
                        </button>
                      ))}
                      <button className="edit" onClick={() => patchStep(sel, { place: undefined })}>Don't move it</button>
                    </div>
                  )}
                </div>
              )}

              <ol className="ws-steps">
                {draft.steps.map((s, i) => (
                  <li key={i} className={sel === i ? 'sel' : ''}>
                    <button className="ws-step" onClick={() => setSel(sel === i ? null : i)}>
                      <em>{i + 1}</em>
                      <span>{stepName(s)}{!s.tileId && <small> · already open</small>}</span>
                      <small>{s.place ? `${screenName(s.place.displayId)} · ${ZONE_LABEL[s.place.zone]}` : 'Opens wherever it likes'}</small>
                    </button>
                    <span className="moves">
                      <button className="edit" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>↑</button>
                      <button className="edit" aria-label="Move down" disabled={i === draft.steps.length - 1} onClick={() => move(i, 1)}>↓</button>
                      <button className="edit" aria-label={`Remove ${stepName(s)}`} onClick={() => { setSteps(draft.steps.filter((_, j) => j !== i)); setSel(null) }}>✕</button>
                    </span>
                  </li>
                ))}
                {!draft.steps.length && <li className="muted">No steps yet. Add a tile below, or capture your current windows.</li>}
              </ol>

              <div className="row">
                <select className="field" aria-label="Tile to add" value={addTile} onChange={(e) => setAddTile(e.target.value)}>
                  <option value="">Add a tile…</option>
                  {tiles.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
                </select>
                <button className="edit" disabled={!addTile} onClick={() => { setSteps([...draft.steps, { tileId: addTile }]); setSel(draft.steps.length); setAddTile('') }}>Add step</button>
              </div>
            </div>
          )}
        </div>

        <footer>
          {draft && draft.id && draft.id !== START_MY_DAY_ID && <button className="edit danger" onClick={() => void remove()}>Delete</button>}
          <span className="spacer" />
          <span className="muted small">{dirty ? 'Unsaved changes' : ''}</span>
          <button className="edit" disabled={!dirty || busy} onClick={() => void save().then((r) => r && toast('Saved', 'ok'))}>Save</button>
          <button className="go" disabled={!draft?.steps.length || busy} onClick={() => void run()}>Run now</button>
        </footer>
      </section>
    </div>
  )
}
