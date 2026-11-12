// Small right-click menus: a generic <Menu> (also used by GroupRail) and the tile menu.
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { START_MY_DAY_ID, type Tile } from '@shared/types'
import { useUi } from '../state'

export interface MenuItem {
  label: string
  run?: () => void
  disabled?: boolean
  danger?: boolean
  /** Draw a divider above this item. */
  sep?: boolean
  submenu?: MenuItem[]
}

interface MenuProps {
  x: number
  y: number
  items: MenuItem[]
  onClose(): void
  label?: string
}

const enabledButtons = (root: HTMLElement | null) =>
  Array.from(root?.querySelectorAll<HTMLButtonElement>(':scope > button:not(:disabled)') ?? [])

function moveFocus(root: HTMLElement | null, dir: 1 | -1 | 'first' | 'last') {
  const items = enabledButtons(root)
  if (!items.length) return
  const i = items.indexOf(document.activeElement as HTMLButtonElement)
  const next = dir === 'first' ? 0 : dir === 'last' ? items.length - 1
    : i < 0 ? (dir === 1 ? 0 : items.length - 1) : (i + dir + items.length) % items.length
  items[next].focus({ preventScroll: true })
}

export function Menu({ x, y, items, onClose, label }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null)
  const subRef = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState({ left: x, top: y })
  const [sub, setSub] = useState<number | null>(null)
  const [subLeft, setSubLeft] = useState(false)
  const opener = useRef<Element | null>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  // Keep inside the window.
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const r = el.getBoundingClientRect()
    setPos({
      left: Math.max(8, Math.min(x, innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(y, innerHeight - r.height - 8)),
    })
  }, [x, y])

  useEffect(() => {
    opener.current = document.activeElement
    moveFocus(ref.current, 'first')
    const close = () => closeRef.current()
    const onDown = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node)) close() }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close() }
    }
    document.addEventListener('pointerdown', onDown, true)
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('scroll', close, true)
    window.addEventListener('blur', close)
    window.addEventListener('resize', close)
    const restore = opener.current
    return () => {
      document.removeEventListener('pointerdown', onDown, true)
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('scroll', close, true)
      window.removeEventListener('blur', close)
      window.removeEventListener('resize', close)
      if (restore instanceof HTMLElement && document.contains(restore) && !document.querySelector('.scrim.open')) {
        restore.focus({ preventScroll: true })
      }
    }
  }, [])

  // Flip the submenu left if it would leave the window.
  useLayoutEffect(() => {
    if (sub === null || !subRef.current || !ref.current) return
    const m = ref.current.getBoundingClientRect()
    setSubLeft(m.right + subRef.current.offsetWidth + 8 > innerWidth)
  }, [sub])

  const activate = (item: MenuItem, i: number) => {
    if (item.disabled) return
    if (item.submenu) { setSub(i); requestAnimationFrame(() => moveFocus(subRef.current, 'first')); return }
    onClose()
    item.run?.()
  }

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const inSub = !!subRef.current?.contains(document.activeElement)
    const root = inSub ? subRef.current : ref.current
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); moveFocus(root, 1); break
      case 'ArrowUp': e.preventDefault(); moveFocus(root, -1); break
      case 'Home': e.preventDefault(); moveFocus(root, 'first'); break
      case 'End': e.preventDefault(); moveFocus(root, 'last'); break
      case 'ArrowRight': {
        if (inSub) break
        const i = Number((document.activeElement as HTMLElement)?.dataset.i)
        if (items[i]?.submenu) { e.preventDefault(); activate(items[i], i) }
        break
      }
      case 'ArrowLeft':
        if (inSub && sub !== null) {
          e.preventDefault()
          const back = ref.current?.querySelector<HTMLButtonElement>(`:scope > button[data-i="${sub}"]`)
          setSub(null); back?.focus({ preventScroll: true })
        }
        break
      case 'Tab': e.preventDefault(); onClose(); break
    }
  }

  // Portal to <body>: rails use backdrop-filter, which would trap a fixed menu inside them.
  return createPortal(
    <div
      ref={ref}
      className="menu"
      role="menu"
      aria-label={label}
      style={{ left: pos.left, top: pos.top }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) => (
        <button
          key={i}
          type="button"
          role="menuitem"
          data-i={i}
          className={`${item.danger ? 'danger' : ''} ${item.sep ? 'sep' : ''} ${sub === i ? 'open' : ''}`}
          disabled={item.disabled}
          aria-haspopup={item.submenu ? 'menu' : undefined}
          aria-expanded={item.submenu ? sub === i : undefined}
          onClick={() => activate(item, i)}
          onMouseEnter={() => setSub(item.submenu ? i : null)}
        >
          <span>{item.label}</span>
          {item.submenu && <i aria-hidden="true">›</i>}
        </button>
      ))}
      {sub !== null && items[sub]?.submenu && (
        <div
          ref={subRef}
          className={`menu sub ${subLeft ? 'left' : ''}`}
          role="menu"
          aria-label={items[sub].label}
          style={{ top: (ref.current?.querySelector<HTMLElement>(`[data-i="${sub}"]`)?.offsetTop ?? 0) - 5 }}
        >
          {items[sub].submenu!.map((s, j) => (
            <button
              key={j}
              type="button"
              role="menuitem"
              disabled={s.disabled}
              onClick={() => { if (s.disabled) return; onClose(); s.run?.() }}
            >
              <span>{s.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>,
    document.body,
  )
}

export function ContextMenu({ tile, x, y, onClose }: { tile: Tile; x: number; y: number; onClose(): void }) {
  const { profile, openTile, setDialog, toast } = useUi()
  const groups = profile?.groups ?? []
  const routine = profile?.routines.find((r) => r.id === START_MY_DAY_ID)
  const steps = routine?.steps.map((s) => s.tileId) ?? []
  const inDay = steps.includes(tile.id)
  const others = groups.filter((g) => g.id !== tile.groupId)

  const moveTo = async (groupId: string, name: string) => {
    const r = await window.bridge.updateTile(tile.id, { groupId })
    toast(r.ok ? `Moved ${tile.name} to ${name}` : r.message, r.ok ? 'ok' : 'error')
  }
  const toggleDay = async () => {
    if (!routine) return
    // Keep every other step (placements, window-only steps) exactly as they are.
    const next = inDay ? routine.steps.filter((s) => s.tileId !== tile.id) : [...routine.steps, { tileId: tile.id }]
    const r = await window.bridge.saveRoutine({ ...routine, steps: next })
    if (!r.ok) toast(r.message, 'error')
    else toast(inDay ? `Removed ${tile.name} from Start my day` : `Added ${tile.name} to Start my day`, 'ok')
  }
  const remove = async () => {
    const r = await window.bridge.deleteTile(tile.id)
    if (!r.ok) { toast(r.message, 'error'); return }
    const saved = r.data ?? tile
    toast(`Deleted ${tile.name}`, 'info', {
      label: 'Undo',
      run: () => {
        void window.bridge.restoreTile(saved).then((u) => { if (!u.ok) toast(u.message, 'error') })
      },
    }, 6000)
  }

  const items: MenuItem[] = [
    { label: 'Open', run: () => void openTile(tile) },
    { label: 'Edit…', run: () => setDialog({ kind: 'edit', tile }) },
    {
      label: 'Move to',
      disabled: others.length === 0,
      submenu: others.map((g) => ({ label: g.name, run: () => void moveTo(g.id, g.name) })),
    },
    { label: inDay ? 'Remove from Start my day' : 'Add to Start my day', run: () => void toggleDay(), disabled: !routine },
    { label: 'Delete', danger: true, sep: true, run: () => void remove() },
  ]
  return <Menu x={x} y={y} items={items} onClose={onClose} label={tile.name} />
}
