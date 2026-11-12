// Shared UI state for every component. Owned by the coordinator; wave agents consume it via useUi().
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { START_MY_DAY_ID, type Profile, type RoutineStatus, type ShellState, type Tile, type ToastTone } from '@shared/types'
import type { Reactor } from './reactor/Reactor'
import { capFor, IDLE_AFTER_MS, QUALITY_BUDGET } from './reactor/frameGate'
import { applyTheme } from './theme'

export interface ToastItem {
  id: number
  message: string
  tone: ToastTone
  action?: { label: string; run: () => void }
}

export interface RoutineView {
  running: boolean
  /** Per-step status keyed by step index. Reset when a new run starts. */
  steps: Record<number, RoutineStatus>
  /** Outcome of the last finished run (undefined until one finishes). */
  ok?: boolean
}

export interface UiState {
  profile: Profile | null
  shell: ShellState | null
  activeGroupId: string
  setActiveGroupId(id: string): void
  /** Set by Core once the canvas exists. May be null early on; always use optional chaining. */
  reactor: React.MutableRefObject<Reactor | null>
  toasts: ToastItem[]
  toast(message: string, tone?: ToastTone, action?: ToastItem['action'], ms?: number): void
  dismissToast(id: number): void
  routine: RoutineView
  startMyDay(): Promise<void>
  /** Opens a tile with reactor feedback and an error toast on failure. */
  openTile(tile: Tile): Promise<void>
  dialog: Dialog
  setDialog(d: Dialog): void
}

export type Dialog =
  | { kind: 'none' }
  | { kind: 'add' }
  | { kind: 'settings' }
  | { kind: 'edit'; tile: Tile }
  | { kind: 'workspaces'; routineId?: string }

const Ctx = createContext<UiState | null>(null)

export function useUi(): UiState {
  const v = useContext(Ctx)
  if (!v) throw new Error('useUi outside <UiProvider>')
  return v
}

/** Fired on window when the palette should take focus (hotkey, tray, typing on home). */
export const FOCUS_PALETTE_EVENT = 'bridge:focus-palette'

export function UiProvider({ children }: { children: ReactNode }) {
  const [profile, setProfile] = useState<Profile | null>(null)
  const [shell, setShell] = useState<ShellState | null>(null)
  const [activeGroupId, setActiveGroupId] = useState('')
  const [toasts, setToasts] = useState<ToastItem[]>([])
  const [routine, setRoutine] = useState<RoutineView>({ running: false, steps: {} })
  const [dialog, setDialog] = useState<Dialog>({ kind: 'none' })
  const reactor = useRef<Reactor | null>(null)
  const nextToast = useRef(1)

  const dismissToast = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), [])
  const toast = useCallback<UiState['toast']>((message, tone = 'info', action, ms) => {
    const id = nextToast.current++
    setToasts((t) => [...t.slice(-3), { id, message, tone, action }])
    setTimeout(() => dismissToast(id), ms ?? (action ? 6000 : 3500))
  }, [dismissToast])

  const startMyDay = useCallback(async () => {
    if (routine.running) return
    const steps = profile?.routines.find((r) => r.id === START_MY_DAY_ID)?.steps ?? []
    if (!steps.length) {
      toast('Start my day has no steps yet. Right-click a tile, then choose Add to Start my day')
      return
    }
    reactor.current?.burst()
    reactor.current?.setProgress(0)
    setRoutine({ running: true, steps: {} })
    // One message for the whole run; it says which step failed, if any.
    const r = await window.bridge.runRoutine(START_MY_DAY_ID)
    if (!r.ok) toast(r.message, 'error')
  }, [routine.running, toast, profile])

  const openTile = useCallback(async (tile: Tile) => {
    reactor.current?.pulse(14)
    const r = await window.bridge.openTile(tile.id)
    if (!r.ok) {
      reactor.current?.fail()
      toast(r.message, 'error')
    }
  }, [toast])

  // Initial load + main → renderer events.
  useEffect(() => {
    window.bridge.getProfile().then(setProfile)
    window.bridge.getShellState().then(setShell)
    return window.bridge.onEvent((e) => {
      switch (e.type) {
        case 'profile': setProfile(e.profile); break
        case 'shell': setShell(e.state); break
        case 'toast': toast(e.message, e.tone); break
        case 'launched': if (!e.ok) reactor.current?.fail(); break
        case 'routine': {
          const p = e.progress
          if (p.status === 'finished') {
            if (p.total === 0) {
              reactor.current?.setProgress(null)
              setRoutine({ running: false, steps: {} })
              break
            }
            reactor.current?.setProgress(1)
            setTimeout(() => {
              reactor.current?.setProgress(null)
              setRoutine((r) => ({ ...r, running: false, ok: p.ok !== false }))
            }, 1200)
          } else {
            // A run started from the tray or `bridge start` also begins at step 0: clear the last run's marks.
            setRoutine((r) => ({
              running: true,
              ok: undefined,
              steps: { ...(p.step === 0 && p.status === 'running' ? {} : r.steps), [p.step]: p.status },
            }))
            if (p.status === 'running') reactor.current?.pulse(10)
            if (p.status === 'done') reactor.current?.setProgress((p.step + 1) / p.total)
            if (p.status === 'failed') reactor.current?.fail()
          }
          break
        }
        case 'command':
          if (e.command === 'focus-palette') window.dispatchEvent(new Event(FOCUS_PALETTE_EVENT))
          if (e.command === 'open-settings') setDialog({ kind: 'settings' })
          if (e.command === 'start-my-day') document.getElementById('start-my-day')?.click()
          break
      }
    })
  }, [toast])

  // Keep a valid active group.
  useEffect(() => {
    if (profile && !profile.groups.some((g) => g.id === activeGroupId)) setActiveGroupId(profile.groups[0]?.id ?? '')
  }, [profile, activeGroupId])

  // Reactor calm + frame cap follow settings, OS reduced-motion, focus and visibility.
  useEffect(() => {
    const mq = matchMedia('(prefers-reduced-motion: reduce)')
    const applyCalm = () => reactor.current?.setCalm(!!profile?.settings.calmMotion || mq.matches)
    // Frame cap: 30 fps focused and active, 12 blurred or idle (45 s without input), paused hidden.
    let focused = document.hasFocus()
    let lastInput = performance.now()
    let idle = false
    const fps = profile?.settings.fps ?? 30
    reactor.current?.setRenderBudget(QUALITY_BUDGET[profile?.settings.quality ?? 'medium'])
    const apply = () => reactor.current?.setFrameCap(capFor(document.hidden, focused, idle, fps))
    const applyThrottle = (e?: Event) => {
      focused = e?.type === 'focus' ? true : e?.type === 'blur' ? false : document.hasFocus()
      apply()
    }
    const onInput = () => {
      lastInput = performance.now()
      if (idle) { idle = false; apply() }
    }
    const idleTimer = window.setInterval(() => {
      const now = performance.now() - lastInput > IDLE_AFTER_MS
      if (now !== idle) { idle = now; apply() }
    }, 5000)
    applyCalm(); applyThrottle()
    mq.addEventListener('change', applyCalm)
    addEventListener('focus', applyThrottle); addEventListener('blur', applyThrottle)
    document.addEventListener('visibilitychange', applyThrottle)
    for (const t of ['pointermove', 'pointerdown', 'keydown', 'wheel'] as const) addEventListener(t, onInput, { passive: true })
    return () => {
      clearInterval(idleTimer)
      mq.removeEventListener('change', applyCalm)
      removeEventListener('focus', applyThrottle); removeEventListener('blur', applyThrottle)
      document.removeEventListener('visibilitychange', applyThrottle)
      for (const t of ['pointermove', 'pointerdown', 'keydown', 'wheel'] as const) removeEventListener(t, onInput)
    }
  }, [profile?.settings.calmMotion, profile?.settings.fps, profile?.settings.quality])

  // Theme follows settings live.
  useEffect(() => {
    if (profile) applyTheme(profile.settings)
  }, [profile?.settings.theme, profile?.settings.accent, profile?.settings.glass])

  const value = useMemo<UiState>(() => ({
    profile, shell, activeGroupId, setActiveGroupId, reactor, toasts, toast, dismissToast,
    routine, startMyDay, openTile, dialog, setDialog,
  }), [profile, shell, activeGroupId, toasts, toast, dismissToast, routine, startMyDay, openTile, dialog])

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}
