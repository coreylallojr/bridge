// Edit a tile's name, target, keyword and group.
import { useEffect, useRef, useState, type FormEvent } from 'react'
import type { Tile } from '@shared/types'
import { useUi } from '../state'

export function EditTileDialog() {
  const { dialog, setDialog } = useUi()
  const open = dialog.kind === 'edit'
  return (
    <div
      className={`scrim ${open ? 'open' : ''}`}
      aria-hidden={!open}
      onMouseDown={(e) => { if (open && e.target === e.currentTarget) setDialog({ kind: 'none' }) }}
    >
      {dialog.kind === 'edit' && (
        <EditPanel key={dialog.tile.id} tile={dialog.tile} onClose={() => setDialog({ kind: 'none' })} />
      )}
    </div>
  )
}

function EditPanel({ tile, onClose }: { tile: Tile; onClose(): void }) {
  const { profile, toast } = useUi()
  const [name, setName] = useState(tile.name)
  const [target, setTarget] = useState(tile.target)
  const [keyword, setKeyword] = useState(tile.keyword ?? '')
  const [groupId, setGroupId] = useState(tile.groupId)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const first = useRef<HTMLInputElement>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  useEffect(() => {
    first.current?.focus()
    first.current?.select()
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !e.defaultPrevented) { e.preventDefault(); e.stopPropagation(); closeRef.current() }
    }
    document.addEventListener('keydown', onKey, true)
    return () => document.removeEventListener('keydown', onKey, true)
  }, [])

  const save = async (e: FormEvent) => {
    e.preventDefault()
    if (busy) return
    if (!name.trim()) { setError('Give it a name'); return }
    if (!target.trim()) { setError('Tell Bridge what to open'); return }
    setBusy(true)
    // An empty keyword means "no keyword".
    const r = await window.bridge.updateTile(tile.id, {
      name: name.trim(), target: target.trim(), keyword: keyword.trim(), groupId,
    })
    setBusy(false)
    if (!r.ok) { setError(r.message); return }
    toast(`Saved ${name.trim()}`, 'ok')
    onClose()
  }

  return (
    <form className="panel edit-panel" role="dialog" aria-modal="true" aria-labelledby="edit-title" onSubmit={save}>
      <header>
        <h2 id="edit-title">Edit tile</h2>
        <p>{tile.name}</p>
        <button type="button" className="x" onClick={onClose}>Close</button>
      </header>
      <div className="form-grid">
        <label htmlFor="edit-name">Name</label>
        <input id="edit-name" ref={first} className="field" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} />

        <label htmlFor="edit-target">Opens</label>
        <input id="edit-target" className="field" spellCheck={false} value={target} onChange={(e) => setTarget(e.target.value)} />

        <label htmlFor="edit-keyword">Keyword</label>
        <div>
          <input id="edit-keyword" className="field" spellCheck={false} value={keyword} maxLength={24} onChange={(e) => setKeyword(e.target.value.replace(/\s+/g, ''))} />
          <small className="kw-hint">A short word for the palette, like kanban</small>
        </div>

        <label htmlFor="edit-group">Group</label>
        <select id="edit-group" className="field" value={groupId} onChange={(e) => setGroupId(e.target.value)}>
          {(profile?.groups ?? []).map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
        </select>
      </div>
      <p className="form-error" role="alert">{error}</p>
      <footer>
        <button type="button" className="edit" onClick={onClose}>Cancel</button>
        <button type="submit" className="go" disabled={busy}>Save</button>
      </footer>
    </form>
  )
}
