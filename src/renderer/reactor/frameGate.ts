// Reactor frame budget: pure helpers (no three.js) so the gate is unit-testable.
// The reactor used ~1.8 CPU cores drawing bloom at the monitor's refresh rate (docs/perf-baseline.md).
// It now draws at most 30 fps while Bridge is focused, 12 fps while blurred, and not at all while hidden.

export const FOCUSED_FPS = 30
export const BLURRED_FPS = 12
/** A cap of 0 means paused: no animation loop at all. */
export const PAUSED = 0

/**
 * RAF timestamps land on vsync, so a frame that is "due" can arrive a hair early. Allow up to 2 ms (at most 10% of the
 * budget), so a 30 fps cap on a 60 Hz monitor draws every other vsync (not every third), while a 165 Hz monitor
 * still stays under the cap.
 */
const SLACK_MS = 2

/** Should the RAF callback at `now` draw, given the last drawn frame at `last` (0 = never drawn)? */
export function shouldDraw(now: number, last: number, capFps: number): boolean {
  if (!(capFps > 0)) return false
  if (!(last > 0) || capFps === Infinity) return true
  const budget = 1000 / capFps
  return now - last >= budget - Math.min(SLACK_MS, budget * 0.1)
}

/** No mouse/keyboard input for this long while focused → treat like blurred (the reactor just idles along). */
export const IDLE_AFTER_MS = 45_000

/** Frame cap for the window's state: hidden → paused, focused and active → 30, blurred or idle → 12. */
export function capFor(hidden: boolean, focused: boolean, idle = false, focusedFps = FOCUSED_FPS): number {
  if (hidden) return PAUSED
  return focused && !idle ? focusedFps : Math.min(BLURRED_FPS, focusedFps)
}

/** Internal render budget per quality setting (pixels). */
export const QUALITY_BUDGET: Record<'low' | 'medium' | 'high', number> = { low: 0.35e6, medium: 0.6e6, high: 1.2e6 }

/** Largest simulation step for a cap: ~1 frame of budget, never below 50 ms nor above 100 ms (no jumps after stalls). */
export function maxStep(capFps: number): number {
  return Math.min(0.1, Math.max(0.05, 1.25 / Math.max(1, capFps)))
}

/** Pixel budget for the reactor's internal render (bloom is the cost; its glow hides upscaling softness). */
export const RENDER_BUDGET_PX = 0.6e6

/**
 * Internal render resolution as a multiple of CSS pixels. The clock and UI are HTML, so they stay sharp; only the
 * glowing 3D scene is rendered at ≤ 0.6 MP and scaled up by the browser. Measured on the user's Intel UHD: full-size
 * bloom kept the WebView2 renderer + GPU process at ~1 core each even at 30 fps.
 */
export function renderRatio(cssW: number, cssH: number, dpr: number, budget = RENDER_BUDGET_PX): number {
  const r = Math.min(dpr > 0 ? dpr : 1, 1.5)
  const px = Math.max(1, cssW * cssH)
  return Math.min(r, Math.sqrt(budget / px))
}
