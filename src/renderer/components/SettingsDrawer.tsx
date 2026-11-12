// Settings page: section list on the left, everything live (no Save button). General (hotkey, where Bridge lives,
// pinned screen), Appearance (theme, accent, glass), Motion (calm, frame rate, quality), Widgets (which + order),
// Your data, and the PowerShell line (shown and copied, never installed).
import { useEffect, useRef, useState } from 'react'
import type { DisplayInfo, DisplayMode, Settings, ThemeName } from '@shared/types'
import { useUi } from '../state'
import { THEMES } from '../theme'
import { WIDGETS, orderedWidgets } from './widgets'

const SECTIONS = [
  { id: 'set-general', label: 'General' },
  { id: 'set-look', label: 'Appearance' },
  { id: 'set-motion', label: 'Motion' },
  { id: 'set-widgets', label: 'Widgets' },
  { id: 'set-data', label: 'Your data' },
  { id: 'set-about', label: 'About' },
] as const

const FPS: Settings['fps'][] = [15, 24, 30, 60]
const QUALITY: { id: Settings['quality']; label: string }[] = [
  { id: 'low', label: 'Light' },
  { id: 'medium', label: 'Balanced' },
  { id: 'high', label: 'Sharp' },
]

const MODES: { id: DisplayMode; label: string }[] = [
  { id: 'auto', label: 'Auto' },
  { id: 'pinned', label: 'Pinned' },
  { id: 'summon', label: 'Summon' },
]

const MODE_HELP: Record<DisplayMode, string> = {
  auto: 'Pinned when you have more than one screen, otherwise summoned with the hotkey.',
  pinned: 'Bridge stays on one screen, behind your other windows.',
  summon: 'Bridge hides until you press the hotkey, then opens on the screen your mouse is on.',
}

const CODE_KEYS: Record<string, string> = {
  Space: 'Space', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert',
  Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown',
  ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'",
  Comma: ',', Period: '.', Slash: '/', Backquote: '`',
  NumpadAdd: 'numadd', NumpadSubtract: 'numsub', NumpadMultiply: 'nummult', NumpadDivide: 'numdiv', NumpadDecimal: 'numdec',
}

/** KeyboardEvent → Electron accelerator ("Alt+Space", "Ctrl+Shift+B"), or null if it isn't a usable combo yet. */
function toAccelerator(e: KeyboardEvent): string | null {
  const c = e.code
  let key: string | undefined
  if (/^Key[A-Z]$/.test(c)) key = c.slice(3)
  else if (/^Digit\d$/.test(c)) key = c.slice(5)
  else if (/^F\d{1,2}$/.test(c)) key = c
  else if (/^Numpad\d$/.test(c)) key = `num${c.slice(6)}`
  else key = CODE_KEYS[c]
  if (!key) return null
  if (!e.ctrlKey && !e.altKey && !e.metaKey) return null // Shift alone would steal normal typing
  return [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super', key].filter(Boolean).join('+')
}

const pretty = (accel: string) => accel.split('+').join(' + ')

export function SettingsDrawer() {
  const { dialog, setDialog, shell, profile } = useUi()
  const open = dialog.kind === 'settings'
  const settings = profile?.settings
  const [capturing, setCapturing] = useState(false)
  const [hotkeyMsg, setHotkeyMsg] = useState<{ text: string; tone: 'ok' | 'warn' } | null>(null)
  const [displays, setDisplays] = useState<DisplayInfo[]>([])
  const [copied, setCopied] = useState(false)
  const closeBtn = useRef<HTMLButtonElement>(null)

  const close = () => { setCapturing(false); setDialog({ kind: 'none' }) }
  const save = (patch: Partial<Settings>) => window.bridge.updateSettings(patch)

  // On open: fresh display list, focus inside the drawer, clear old messages.
  useEffect(() => {
    if (!open) return
    setHotkeyMsg(null)
    setCopied(false)
    window.bridge.listDisplays().then(setDisplays).catch(() => setDisplays([]))
    const id = setTimeout(() => closeBtn.current?.focus(), 60)
    return () => clearTimeout(id)
  }, [open])

  // Keys while open: hotkey capture, or Esc to close. Capture phase so nothing else sees them first.
  useEffect(() => {
    if (!open) return
    const onKey = async (e: KeyboardEvent) => {
      if (capturing) {
        e.preventDefault()
        e.stopPropagation()
        if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.metaKey) { setCapturing(false); return }
        const accel = toAccelerator(e)
        if (!accel) return
        setCapturing(false)
        const r = await save({ hotkey: accel })
        if (!r.ok) setHotkeyMsg({ text: r.message, tone: 'warn' })
        else if (r.data && r.data.hotkeyOk === false) setHotkeyMsg({ text: 'That shortcut is taken by another app. Try a different one.', tone: 'warn' })
        else setHotkeyMsg({ text: `Saved. Press ${pretty(accel)} to bring up Bridge.`, tone: 'ok' })
        return
      }
      if (e.key === 'Escape') { e.preventDefault(); close() }
    }
    addEventListener('keydown', onKey, true)
    return () => removeEventListener('keydown', onKey, true)
  }) // re-bound every render so it sees the current `capturing`

  const copyLine = async () => {
    if (!shell) return
    await window.bridge.copyText(shell.profileLine)
    setCopied(true)
    setTimeout(() => setCopied(false), 1600)
  }

  const hotkey = shell?.hotkey ?? ''
  const takenNow = shell?.hotkeyOk === false && !hotkeyMsg && !capturing

  return (
    <div className={`scrim drawer-scrim${open ? ' open' : ''}`} inert={!open} onMouseDown={(e) => { if (e.target === e.currentTarget) close() }}>
      <section className="panel drawer settings-page" role="dialog" aria-modal="true" aria-label="Settings">
        <header>
          <h2>Settings</h2>
          <button ref={closeBtn} className="x" onClick={close}>Close</button>
        </header>

        <div className="settings-grid">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map((x) => (
            <button key={x.id} onClick={() => document.getElementById(x.id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })}>{x.label}</button>
          ))}
          <button className="ws-link" onClick={() => setDialog({ kind: 'workspaces' })}>Start my day &amp; workspaces →</button>
        </nav>
        <div className="drawer-body">
          <h4 id="set-general" className="set-head">General</h4>
          <div className="set">
            <h3>Hotkey</h3>
            <p className="muted">Brings Bridge up from anywhere.</p>
            <div className="row">
              <div className={`field keycap${capturing ? ' capturing' : ''}`} aria-live="polite">
                {capturing ? 'Press the new shortcut…' : hotkey ? <kbd>{pretty(hotkey)}</kbd> : 'None'}
              </div>
              <button className="edit" onClick={() => { setHotkeyMsg(null); setCapturing((c) => !c) }}>
                {capturing ? 'Cancel' : 'Change'}
              </button>
            </div>
            {capturing && <p className="muted small">Hold Ctrl or Alt and press a key. Esc cancels.</p>}
            {hotkeyMsg && <p className={`note ${hotkeyMsg.tone}`}>{hotkeyMsg.text}</p>}
            {takenNow && <p className="note warn">{pretty(hotkey)} is taken by another app. Try a different one.</p>}
          </div>

          <div className="set">
            <h3>Where Bridge lives</h3>
            <div className="seg" role="radiogroup" aria-label="Display mode">
              {MODES.map((m) => (
                <button
                  key={m.id}
                  role="radio"
                  aria-checked={settings?.mode === m.id}
                  className={settings?.mode === m.id ? 'on' : ''}
                  onClick={() => void save({ mode: m.id })}
                >
                  {m.label}
                </button>
              ))}
            </div>
            {settings && <p className="muted small">{MODE_HELP[settings.mode]}</p>}
          </div>

          <div className="set">
            <h3>Pinned screen</h3>
            <div className="radios" role="radiogroup" aria-label="Pinned screen">
              {[{ id: null as number | null, label: 'Choose automatically' }, ...displays].map((d) => {
                const on = (settings?.pinnedDisplayId ?? null) === d.id
                return (
                  <button key={d.id ?? 'auto'} role="radio" aria-checked={on} className={`radio${on ? ' on' : ''}`}
                    onClick={() => void save({ pinnedDisplayId: d.id })}>
                    <span className="pip" />{d.label}
                    {'primary' in d && d.primary && <small>main screen</small>}
                  </button>
                )
              })}
            </div>
          </div>

          <h4 id="set-look" className="set-head">Appearance</h4>
          <div className="set">
            <h3>Theme</h3>
            <div className="themes" role="radiogroup" aria-label="Theme">
              {(Object.keys(THEMES) as ThemeName[]).map((t) => {
                const th = THEMES[t]
                const on = settings?.theme === t
                return (
                  <button key={t} role="radio" aria-checked={on} className={`theme-card${on ? ' on' : ''}`}
                    style={{ '--c': th.line, '--bg': th.sea } as React.CSSProperties}
                    onClick={() => void save({ theme: t, accent: null })}>
                    <span className="swatch" />
                    <b>{th.label}</b>
                    <small>{th.blurb}</small>
                  </button>
                )
              })}
            </div>
          </div>
          <div className="set">
            <h3>Accent color</h3>
            <div className="row">
              <input type="color" className="color" aria-label="Accent color"
                value={settings?.accent ?? THEMES[settings?.theme ?? 'command'].line}
                onChange={(e) => void save({ accent: e.target.value })} />
              <span className="muted small">{settings?.accent ? 'Custom accent' : 'From the theme'}</span>
              {settings?.accent && <button className="edit" onClick={() => void save({ accent: null })}>Use theme color</button>}
            </div>
          </div>
          <div className="set">
            <h3>Glass</h3>
            <div className="row">
              <span className="muted small">Solid</span>
              <input type="range" min={0} max={100} step={5} className="range" aria-label="Glass strength"
                value={settings?.glass ?? 60} onChange={(e) => void save({ glass: Number(e.target.value) })} />
              <span className="muted small">See-through</span>
            </div>
          </div>

          <h4 id="set-motion" className="set-head">Motion</h4>
          <div className="set">
            <button className={`toggle${settings?.calmMotion ? ' on' : ''}`} role="switch" aria-checked={!!settings?.calmMotion}
              onClick={() => void save({ calmMotion: !settings?.calmMotion })}>
              <span className="knob" />
              <span>Calm motion</span>
              <small>{settings?.calmMotion ? 'The reactor holds still' : 'The reactor moves'}</small>
            </button>
          </div>
          <div className="set">
            <h3>Reactor frame rate</h3>
            <div className="seg" role="radiogroup" aria-label="Frame rate">
              {FPS.map((f) => (
                <button key={f} role="radio" aria-checked={settings?.fps === f} className={settings?.fps === f ? 'on' : ''}
                  onClick={() => void save({ fps: f })}>{f} fps</button>
              ))}
            </div>
            <p className="muted small">Lower is lighter on your PC. It always drops to 12 fps in the background and stops when hidden.</p>
          </div>
          <div className="set">
            <h3>Reactor quality</h3>
            <div className="seg" role="radiogroup" aria-label="Quality">
              {QUALITY.map((q) => (
                <button key={q.id} role="radio" aria-checked={settings?.quality === q.id} className={settings?.quality === q.id ? 'on' : ''}
                  onClick={() => void save({ quality: q.id })}>{q.label}</button>
              ))}
            </div>
          </div>

          <h4 id="set-widgets" className="set-head">Widgets</h4>
          <div className="set">
            <p className="muted">Pick what shows in the side column, and the order.</p>
            <ul className="widget-list">
              {orderedWidgets(settings?.widgets ?? []).map((w) => {
                const list = settings?.widgets ?? []
                const on = list.includes(w)
                const i = list.indexOf(w)
                const move = (d: number) => {
                  const next = [...list]
                  const j = i + d
                  if (i < 0 || j < 0 || j >= next.length) return
                  ;[next[i], next[j]] = [next[j], next[i]]
                  void save({ widgets: next })
                }
                return (
                  <li key={w} className={on ? 'on' : ''}>
                    <button className={`toggle${on ? ' on' : ''}`} role="switch" aria-checked={on}
                      onClick={() => void save({ widgets: on ? list.filter((x) => x !== w) : [...list, w] })}>
                      <span className="knob" /><span>{WIDGETS[w].label}</span><small>{WIDGETS[w].blurb}</small>
                    </button>
                    {on && <span className="moves">
                      <button className="edit" aria-label={`Move ${WIDGETS[w].label} up`} disabled={i === 0} onClick={() => move(-1)}>↑</button>
                      <button className="edit" aria-label={`Move ${WIDGETS[w].label} down`} disabled={i === list.length - 1} onClick={() => move(1)}>↓</button>
                    </span>}
                  </li>
                )
              })}
            </ul>
          </div>

          <h4 id="set-data" className="set-head">Your data</h4>
          <div className="set">
            <h3>Your data</h3>
            <p className="muted">Tiles, groups and settings are saved on this PC only.</p>
            <div className="row"><button className="edit" onClick={() => void window.bridge.openDataFolder()}>Open data folder</button></div>
          </div>

          <div className="set">
            <h3>Use Bridge from PowerShell</h3>
            <p className="muted">Add this line to your PowerShell profile to type <code>bridge</code> anywhere:</p>
            <div className="codebox">
              <code>{shell?.profileLine ?? ''}</code>
              <button className="edit" onClick={() => void copyLine()}>{copied ? 'Copied' : 'Copy'}</button>
            </div>
          </div>
          <div className="set">
            <h3>Open Bridge when you sign in</h3>
            <p className="muted">
              Press <kbd>Win</kbd>+<kbd>R</kbd>, type <code>shell:startup</code>, and put a shortcut to Bridge in that folder.
              Bridge doesn't add itself, so nothing changes on your PC without you.
            </p>
          </div>

          <h4 id="set-about" className="set-head">About</h4>
          <div className="set">
            <h3>About Bridge</h3>
            <p className="muted">
              Bridge by Corey Lallo · github.com/coreylallojr/bridge<br />
              PolyForm Noncommercial 1.0.0: free for personal, noncommercial use with credit. Commercial use needs permission.
            </p>
          </div>
        </div>
        </div>
      </section>
    </div>
  )
}
