// Themes: presets + optional accent + glass strength, applied live as CSS variables on <html>. The reactor follows the
// accent through a hue-rotate filter on its canvas (its bloom still renders on pure black, as the design requires).
import type { Settings, ThemeName } from '@shared/types'

interface Preset {
  label: string
  blurb: string
  line: string
  amber: string
  sea: string
  text: string
  muted: string
  /** Extra saturation for the reactor (1 = as designed). */
  saturate: number
}

/** The reactor's own cyan, the hue every accent is measured against. */
const REACTOR_HUE = 185

export const THEMES: Record<ThemeName, Preset> = {
  command: { label: 'Command', blurb: 'Deep sea, cyan and amber', line: '#3FE0EE', amber: '#FFB547', sea: '#04121A', text: '#D9F7FB', muted: '#6E9AA6', saturate: 1 },
  ember: { label: 'Ember', blurb: 'Warm amber on dark brown', line: '#FFB547', amber: '#3FE0EE', sea: '#140C05', text: '#FBEEDD', muted: '#A88F72', saturate: 1 },
  mono: { label: 'Mono', blurb: 'Quiet greys, little color', line: '#C3D2D8', amber: '#E3C27F', sea: '#0A0D0F', text: '#E8EDEF', muted: '#7F8B90', saturate: 0.18 },
  contrast: { label: 'High contrast', blurb: 'Pure black, brighter text', line: '#7DF9FF', amber: '#FFD27A', sea: '#000000', text: '#FFFFFF', muted: '#BFD6DB', saturate: 1.15 },
}

const hexRgb = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/** Hue in degrees (0–360) of a #rrggbb color. */
export function hueOf(hex: string): number {
  const [r, g, b] = hexRgb(hex).map((v) => v / 255)
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min
  if (d === 0) return 0
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return (h * 60 + 360) % 360
}

/** CSS variables for a settings object (pure: tested without a DOM). */
export function themeVars(s: Pick<Settings, 'theme' | 'accent' | 'glass'>): Record<string, string> {
  const p = THEMES[s.theme] ?? THEMES.command
  const line = s.accent ?? p.line
  const [r, g, b] = hexRgb(line)
  const [sr, sg, sb] = hexRgb(p.sea)
  const glass = Math.max(0, Math.min(100, s.glass)) / 100
  const rotate = Math.round(((hueOf(line) - REACTOR_HUE + 540) % 360) - 180)
  const filters = [Math.abs(rotate) > 3 ? `hue-rotate(${rotate}deg)` : '', p.saturate !== 1 ? `saturate(${p.saturate})` : ''].filter(Boolean)
  return {
    '--line': line,
    '--line-rgb': `${r}, ${g}, ${b}`,
    '--amber': p.amber,
    '--sea': p.sea,
    '--text': p.text,
    '--muted': p.muted,
    // Glass 0 = nearly solid panels, 100 = very see-through with more blur.
    '--glass': `rgba(${sr}, ${sg}, ${sb}, ${(0.75 - glass * 0.45).toFixed(2)})`,
    '--fill': `rgba(${sr}, ${sg}, ${sb}, ${(0.85 - glass * 0.4).toFixed(2)})`,
    '--panel-bg': `rgba(${sr + 2}, ${sg + 8}, ${sb + 10}, ${(0.97 - glass * 0.15).toFixed(2)})`,
    '--blur': `${Math.round(2 + glass * 10)}px`,
    '--reactor-filter': filters.length ? filters.join(' ') : 'none',
  }
}

export function applyTheme(s: Pick<Settings, 'theme' | 'accent' | 'glass'>): void {
  const root = document.documentElement.style
  for (const [k, v] of Object.entries(themeVars(s))) root.setProperty(k, v)
}
