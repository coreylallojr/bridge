// "Jump to anything": the top-center command palette over tiles, routines and actions.
import { useEffect, useMemo, useRef, useState } from 'react'
import { rank } from '@shared/match'
import { START_MY_DAY_ID, type DisplayMode } from '@shared/types'
import { FOCUS_PALETTE_EVENT, useUi } from '../state'
import { isEditable } from '../dropPaste'
import { KIND_LABEL } from './TileCard'
import { NEW_GROUP_EVENT } from './GroupRail'

interface Entry {
  key: string
  label: string
  sub: string
  tag: string
  fields: string[]
  run(): void
}

const MAX = 8

/**
 * The human part of a target, for matching. Raw targets make fuzzy matching noisy: an app ID like
 * `…8wekyb3d8bbwe!App` matched "ka" for Calculator and Notepad.
 */
function readableTarget(kind: string, target: string): string {
  if (kind === 'url') {
    try { const u = new URL(target); return u.hostname.replace(/^www\./, '') + u.pathname.replace(/\/$/, '') } catch { return '' }
  }
  if (/^shell:/i.test(target) || /^[a-z][a-z0-9+.-]+:(?![\\/])/i.test(target)) return ''
  return target.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''
}

export function Palette() {
  const { profile, shell, dialog, setDialog, startMyDay, openTile, toast } = useUi()
  const [query, setQuery] = useState('')
  const [sel, setSel] = useState(0)
  const [focused, setFocused] = useState(false)
  const input = useRef<HTMLInputElement>(null)
  const dialogOpen = useRef(false)
  dialogOpen.current = dialog.kind !== 'none'

  const entries = useMemo<Entry[]>(() => {
    if (!profile) return []
    const groupName = new Map(profile.groups.map((g) => [g.id, g.name]))
    const setMode = (mode: DisplayMode, label: string) => async () => {
      const r = await window.bridge.updateSettings({ mode })
      toast(r.ok ? `${label} on` : r.message, r.ok ? 'ok' : 'error')
    }
    const calm = profile.settings.calmMotion
    const tiles: Entry[] = profile.tiles.map((t) => ({
      key: t.id,
      label: t.name,
      sub: [t.subtitle, groupName.get(t.groupId)].filter(Boolean).join(' · '),
      tag: KIND_LABEL[t.kind] ?? t.kind,
      fields: [t.name, t.keyword ?? '', t.subtitle ?? '', readableTarget(t.kind, t.target)],
      run: () => void openTile(t),
    }))
    const routines: Entry[] = profile.routines.map((r) => ({
      key: r.id,
      label: r.name,
      sub: r.steps.length ? `${r.steps.length} ${r.steps.length === 1 ? 'step' : 'steps'}` : 'No steps yet',
      tag: 'Routine',
      fields: [r.name],
      run: () => {
        if (r.id === START_MY_DAY_ID) void startMyDay()
        else void window.bridge.runRoutine(r.id).then((x) => { if (!x.ok) toast(x.message, 'error') })
      },
    }))
    const mode = shell?.mode
    const actions: Entry[] = [
      { key: 'a-add', label: 'Add a link or app', sub: 'Paste a link or pick an app', fields: ['Add a link or app', 'new tile'], run: () => setDialog({ kind: 'add' }) },
      { key: 'a-group', label: 'New group', sub: 'An empty group in the rail', fields: ['New group'], run: () => window.dispatchEvent(new Event(NEW_GROUP_EVENT)) },
      { key: 'a-settings', label: 'Settings', sub: 'Hotkey, screen and motion', fields: ['Settings', 'preferences', 'hotkey'], run: () => setDialog({ kind: 'settings' }) },
      {
        key: 'a-calm',
        label: calm ? 'Calm motion off' : 'Calm motion on',
        sub: calm ? 'Bring the reactor back to life' : 'A still glow instead of motion',
        fields: [calm ? 'Calm motion off' : 'Calm motion on', 'calm motion'],
        run: async () => {
          const r = await window.bridge.updateSettings({ calmMotion: !calm })
          toast(r.ok ? (calm ? 'Calm motion off' : 'Calm motion on') : r.message, r.ok ? 'ok' : 'error')
        },
      },
      { key: 'a-pinned', label: 'Pinned mode', sub: mode === 'pinned' ? 'On now' : 'Stay on one screen, behind other windows', fields: ['Pinned mode'], run: setMode('pinned', 'Pinned mode') },
      { key: 'a-summon', label: 'Summon mode', sub: mode === 'summon' ? 'On now' : 'Hide until you press the hotkey', fields: ['Summon mode'], run: setMode('summon', 'Summon mode') },
    ].map((a) => ({ ...a, tag: 'Action', run: () => void a.run() }))
    return [...tiles, ...routines, ...actions]
  }, [profile, shell?.mode, openTile, startMyDay, setDialog, toast])

  const results = useMemo(() => rank(query, entries, (e) => e.fields).slice(0, MAX), [query, entries])
  const open = focused && query.trim() !== ''

  useEffect(() => { setSel(0) }, [query])

  // Hotkey / tray focus, and "just start typing" on the home screen.
  useEffect(() => {
    const focus = () => { input.current?.focus(); input.current?.select() }
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey || e.key.length !== 1 || e.key === ' ') return
      if (dialogOpen.current || isEditable(document.activeElement)) return
      if (document.activeElement?.closest('.menu, .scrim')) return
      input.current?.focus() // the key press lands in the field
    }
    window.addEventListener(FOCUS_PALETTE_EVENT, focus)
    document.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener(FOCUS_PALETTE_EVENT, focus)
      document.removeEventListener('keydown', onKey)
    }
  }, [])

  const run = (e: Entry | undefined) => {
    if (!e) return
    setQuery('')
    input.current?.blur()
    e.run()
  }

  const hotkey = (shell?.hotkey ?? 'Alt+Space').replace(/\+/g, ' ')

  return (
    <div className="palette">
      <div className={`search ${open ? 'open' : ''}`} onMouseDown={(e) => { if (e.target !== input.current) { e.preventDefault(); input.current?.focus() } }}>
        <svg className="glass" viewBox="0 0 20 20" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" /><path d="M13 13l4.5 4.5" /></svg>
        <input
          ref={input}
          type="text"
          spellCheck={false}
          autoComplete="off"
          placeholder="Jump to anything"
          aria-label="Jump to anything"
          role="combobox"
          aria-expanded={open}
          aria-controls="palette-results"
          aria-activedescendant={open && results[sel] ? `pr-${sel}` : undefined}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown' && results.length) { e.preventDefault(); setSel((s) => (s + 1) % results.length) }
            else if (e.key === 'ArrowUp' && results.length) { e.preventDefault(); setSel((s) => (s - 1 + results.length) % results.length) }
            else if (e.key === 'Enter') { e.preventDefault(); run(results[sel]) }
            else if (e.key === 'Escape') {
              if (query) { e.preventDefault(); e.stopPropagation(); setQuery('') }
              else input.current?.blur()
            }
          }}
        />
        <kbd>{hotkey}</kbd>
      </div>
      {open && (
        <div className="results" id="palette-results" role="listbox" aria-label="Results" onMouseDown={(e) => e.preventDefault()}>
          {results.length === 0 && <div className="none">No matches</div>}
          {results.map((r, i) => (
            <div
              key={r.key}
              id={`pr-${i}`}
              role="option"
              aria-selected={i === sel}
              className={`row ${i === sel ? 'sel' : ''}`}
              onMouseMove={() => { if (i !== sel) setSel(i) }}
              onClick={() => run(r)}
            >
              <span className="t"><b>{r.label}</b>{r.sub && <small>{r.sub}</small>}</span>
              <em>{r.tag}</em>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
