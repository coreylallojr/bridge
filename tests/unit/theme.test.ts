import { describe, expect, it } from 'vitest'
import { hueOf, themeVars } from '../../src/renderer/theme'

describe('themeVars', () => {
  it('Command theme leaves the reactor as designed', () => {
    const v = themeVars({ theme: 'command', glass: 60 })
    expect(v['--line']).toBe('#3FE0EE')
    expect(v['--line-rgb']).toBe('63, 224, 238')
    expect(v['--reactor-filter']).toBe('none')
  })
  it('a custom accent recolors the UI and turns the reactor toward it', () => {
    const v = themeVars({ theme: 'command', accent: '#ffb547', glass: 60 })
    expect(v['--line-rgb']).toBe('255, 181, 71')
    expect(v['--reactor-filter']).toMatch(/^hue-rotate\(-1[45]\ddeg\)$/)
  })
  it('Mono desaturates the reactor; glass changes panel opacity and blur', () => {
    expect(themeVars({ theme: 'mono', glass: 60 })['--reactor-filter']).toContain('saturate(0.18)')
    const solid = themeVars({ theme: 'command', glass: 0 })
    const clear = themeVars({ theme: 'command', glass: 100 })
    expect(solid['--blur']).toBe('2px')
    expect(clear['--blur']).toBe('12px')
    expect(solid['--glass']).not.toBe(clear['--glass'])
  })
  it('hueOf', () => {
    expect(Math.round(hueOf('#ff0000'))).toBe(0)
    expect(Math.round(hueOf('#00ff00'))).toBe(120)
    expect(Math.round(hueOf('#3FE0EE'))).toBe(185)
  })
})
