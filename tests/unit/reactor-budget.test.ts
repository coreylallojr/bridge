import { describe, expect, it } from 'vitest'
import {
  BLURRED_FPS, FOCUSED_FPS, PAUSED, capFor, maxStep, renderRatio, shouldDraw,
} from '../../src/renderer/reactor/frameGate'

/** Simulate RAF at `hz` for `ms` with the gate at `cap`; returns the number of drawn frames. */
function simulate(hz: number, cap: number, ms = 1000, jitter = 0): number {
  let last = 0, drawn = 0
  const vsync = 1000 / hz
  for (let i = 1; i * vsync <= ms; i++) {
    const now = 1000 + i * vsync + (jitter ? (i % 2 ? jitter : -jitter) : 0)
    if (shouldDraw(now, last, cap)) { last = now; drawn++ }
  }
  return drawn
}

describe('shouldDraw', () => {
  it('first frame always draws', () => {
    expect(shouldDraw(5, 0, 30)).toBe(true)
    expect(shouldDraw(123456, 0, 12)).toBe(true)
  })

  it('60 Hz at a 30 fps cap draws every other frame', () => {
    const vsync = 1000 / 60
    let last = 1000
    const pattern: boolean[] = []
    for (let i = 1; i <= 6; i++) {
      const now = 1000 + i * vsync
      const d = shouldDraw(now, last, 30)
      pattern.push(d)
      if (d) last = now
    }
    expect(pattern).toEqual([false, true, false, true, false, true])
    expect(simulate(60, 30)).toBeGreaterThanOrEqual(29)
    expect(simulate(60, 30)).toBeLessThanOrEqual(31)
  })

  it('tolerates vsync jitter (a frame arriving 1 ms early still draws)', () => {
    expect(shouldDraw(1000 + 2 * (1000 / 60) - 1, 1000, 30)).toBe(true)
    const n = simulate(60, 30, 1000, 0.5)
    expect(n).toBeGreaterThanOrEqual(29)
    expect(n).toBeLessThanOrEqual(31)
  })

  it('12 fps cap: every 5th frame at 60 Hz', () => {
    const n = simulate(60, 12)
    expect(n).toBeGreaterThanOrEqual(11)
    expect(n).toBeLessThanOrEqual(13)
    expect(shouldDraw(1000 + 4 * (1000 / 60), 1000, 12)).toBe(false)
    expect(shouldDraw(1000 + 5 * (1000 / 60), 1000, 12)).toBe(true)
  })

  it('high-refresh monitors stay at or under the cap', () => {
    for (const hz of [75, 120, 144, 165, 240]) {
      const n = simulate(hz, 30)
      expect(n).toBeLessThanOrEqual(31)
      expect(n).toBeGreaterThanOrEqual(24)
    }
  })

  it('cap 0 (paused) never draws', () => {
    expect(shouldDraw(5000, 0, 0)).toBe(false)
    expect(shouldDraw(5000, 1000, PAUSED)).toBe(false)
  })
})

describe('capFor', () => {
  it('focused 30, blurred 12, hidden paused', () => {
    expect(capFor(false, true)).toBe(FOCUSED_FPS)
    expect(FOCUSED_FPS).toBe(30)
    expect(capFor(false, false)).toBe(BLURRED_FPS)
    expect(BLURRED_FPS).toBe(12)
    expect(capFor(true, true)).toBe(PAUSED)
    expect(capFor(true, false)).toBe(PAUSED)
  })
})

describe('maxStep', () => {
  it('covers one frame at the cap, clamped to 50–100 ms', () => {
    expect(maxStep(30)).toBeCloseTo(0.05)
    expect(maxStep(12)).toBeCloseTo(0.1)
    expect(maxStep(60)).toBeCloseTo(0.05)
    expect(maxStep(0)).toBeCloseTo(0.1)
  })
})

describe('renderRatio', () => {
  it('keeps the internal render at or under the 0.6 MP budget', () => {
    for (const [w, h] of [[1080, 1920], [1920, 1080], [1280, 720], [1600, 900]]) {
      const r = renderRatio(w, h, 1)
      expect(w * h * r * r).toBeLessThanOrEqual(0.6e6 + 1)
    }
  })
  it('small canvases are not upscaled past the device ratio (max 1.5×)', () => {
    expect(renderRatio(400, 300, 1)).toBe(1)
    expect(renderRatio(400, 300, 2)).toBe(1.5)
  })
})
