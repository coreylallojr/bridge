// "+ Add": paste a link/path with a live "Looks like" readout, or pick an installed app.
import { useEffect, useMemo, useRef, useState } from 'react'
import { rank } from '@shared/match'
import type { AppEntry } from '@shared/types'
import { useUi } from '../state'
import { useAddItem } from '../dropPaste'
import { KIND_LABEL } from './TileCard'

// The app list can take a few seconds the first time; keep it for the session.
let appCache: AppEntry[] | null = null

type Readout = { tone: 'ok' | 'error'; text: string } | null

export function AddDialog() {
  const { profile, activeGroupId, dialog, setDialog } = useUi()
  const open = dialog.kind === 'add'
  return (
    <div
      className={`scrim ${open ? 'open' : ''}`}
      aria-hidden={!open}
      onMouseDown={(e) => { if (open && e.target === e.currentTarget) setDialog({ kind: 'none' }) }}
    >
      {open && profile && <AddPanel defaultGroup={activeGroupId} onClose={() => setDialog({ kind: 'none' })} />}
    </div>
  )
}

function AddPanel({ defaultGroup, onClose }: { defaultGroup: string; onClose(): void }) {
  const { profile } = useUi()
  const add = useAddItem()
  const groups = profile?.groups ?? []
  const [groupId, setGroupId] = useState(defaultGroup || groups[0]?.id || '')
  const [input, setInput] = useState('')
  const [readout, setReadout] = useState<Readout>(null)
  const [busy, setBusy] = useState(false)
  const [apps, setApps] = useState<AppEntry[] | null>(appCache)
  const [appQuery, setAppQuery] = useState('')
  const [added, setAdded] = useState<Set<string>>(new Set())
  const seq = useRef(0)
  const first = useRef<HTMLInputElement>(null)
  const groupName = groups.find((g) => g.id === groupId)?.name ?? 'Bridge'
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  useEffect(() => {
    first.current?.focus()
    let alive = true
    window.bridge.listApps().then((a) => { appCache = a; if (alive) setApps(a) }, () => { if (alive) setApps((x) => x ?? []) })
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !e.defaultPrevented) { e.preventDefault(); e.stopPropagation(); closeRef.current() }
    }
    document.addEventListener('keydown', onKey, true)
    return () => { alive = false; document.removeEventListener('keydown', onKey, true) }
  }, [])

  // Live "Looks like" readout, debounced.
  useEffect(() => {
    const text = input.trim()
    const n = ++seq.current
    if (!text) { setReadout(null); return }
    const t = setTimeout(async () => {
      const r = await window.bridge.addItem({ input: text, groupId, dryRun: true })
      if (n !== seq.current) return
      if (r.ok && r.data) {
        const kind = KIND_LABEL[r.data.kind] ?? 'Item'
        const detail = r.data.subtitle && r.data.subtitle !== kind ? r.data.subtitle : r.data.name
        setReadout({ tone: 'ok', text: `Looks like: ${kind}${detail ? ` · ${detail}` : ''}` })
      } else {
        setReadout({ tone: 'error', text: r.message })
      }
    }, 200)
    return () => clearTimeout(t)
  }, [input, groupId])

  const save = async () => {
    const text = input.trim()
    if (!text || busy) return
    setBusy(true)
    const ok = await add({ input: text, groupId })
    setBusy(false)
    if (ok) onClose()
  }

  const shown = useMemo(() => {
    if (!apps) return []
    return appQuery.trim() ? rank(appQuery, apps, (a) => [a.name]) : apps
  }, [apps, appQuery])

  const pickApp = async (app: AppEntry) => {
    if (added.has(app.appId)) return
    const ok = await add({ input: 'shell:AppsFolder\\' + app.appId, name: app.name, groupId })
    if (ok) setAdded((s) => new Set(s).add(app.appId))
  }

  return (
    <div className="panel add-panel" role="dialog" aria-modal="true" aria-labelledby="add-title">
      <header>
        <h2 id="add-title">Add to {groupName}</h2>
        <p>Or just drop it on Bridge.</p>
        <button type="button" className="x" onClick={onClose}>Close</button>
      </header>

      <section className="add-paste">
        <label htmlFor="add-input">Paste a link, or a file or folder path</label>
        <div className="row">
          <input
            id="add-input"
            ref={first}
            className="field"
            spellCheck={false}
            autoComplete="off"
            placeholder="https://… or C:\…"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void save() } }}
          />
          <button type="button" className="go" disabled={!input.trim() || busy} onClick={() => void save()}>Add</button>
        </div>
        <p className={`readout-line ${readout?.tone ?? ''}`} aria-live="polite">{readout?.text ?? '\u00a0'}</p>
      </section>

      <section className="add-apps">
        <div className="apps-head">
          <h3>Or pick an app</h3>
          <input
            className="field"
            aria-label="Search apps"
            placeholder="Search apps"
            spellCheck={false}
            value={appQuery}
            onChange={(e) => setAppQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && shown[0]) { e.preventDefault(); void pickApp(shown[0]) }
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                e.currentTarget.closest('.add-apps')?.querySelector<HTMLButtonElement>('.app-list button')?.focus()
              }
            }}
          />
        </div>
        <div
          className="app-list"
          role="list"
          onKeyDown={(e) => {
            if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
            e.preventDefault()
            const el = (e.key === 'ArrowDown'
              ? (document.activeElement as HTMLElement)?.nextElementSibling
              : (document.activeElement as HTMLElement)?.previousElementSibling) as HTMLElement | null
            el?.focus()
          }}
        >
          {apps === null && <p className="muted">Loading apps…</p>}
          {apps !== null && shown.length === 0 && <p className="muted">{apps.length ? 'No apps match' : 'No apps found'}</p>}
          {shown.map((a) => {
            const done = added.has(a.appId)
            return (
              <button key={a.appId} type="button" role="listitem" className={done ? 'done' : ''} onClick={() => void pickApp(a)}>
                <span>{a.name}</span>
                <em>{done ? 'Added' : 'Add'}</em>
              </button>
            )
          })}
        </div>
      </section>

      <footer>
        <label className="group-pick">
          Add to
          <select className="field" value={groupId} onChange={(e) => setGroupId(e.target.value)}>
            {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
          </select>
        </label>
        <button type="button" className="edit" onClick={onClose}>Done</button>
      </footer>
    </div>
  )
}
