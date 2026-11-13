// Bridge landing page. Runs the app's own reactor (src/renderer/reactor/Reactor.ts) and themes (src/renderer/theme.ts)
// under a JARVIS-style HUD with a flickering SVG arc reactor. Nothing here talks to the network.
import { Reactor } from '../src/renderer/reactor/Reactor'
import { THEMES, themeVars } from '../src/renderer/theme'

const $ = <T extends HTMLElement = HTMLElement>(s: string) => document.querySelector(s) as T
const $$ = <T extends HTMLElement = HTMLElement>(s: string) => [...document.querySelectorAll(s)] as T[]
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
const SVG = 'http://www.w3.org/2000/svg'
const rnd = (a: number, b: number) => a + Math.random() * (b - a)

// ---------------------------------------------------------------- SVG arc reactor: ticks, coils, arcs of lightning
const arc = $('#arc') as unknown as SVGSVGElement
const ticks = arc.querySelector('.ticks')!
for (let i = 0; i < 72; i++) {
  const a = (i / 72) * Math.PI * 2, major = i % 6 === 0, r1 = 92, r2 = major ? 100 : 96
  const l = document.createElementNS(SVG, 'line')
  l.setAttribute('x1', String(Math.cos(a) * r1)); l.setAttribute('y1', String(Math.sin(a) * r1))
  l.setAttribute('x2', String(Math.cos(a) * r2)); l.setAttribute('y2', String(Math.sin(a) * r2))
  if (major) l.setAttribute('class', 'major')
  ticks.appendChild(l)
}
const coils = arc.querySelector('.coils')!
for (let i = 0; i < 10; i++) {
  const deg = i * 36
  const g = document.createElementNS(SVG, 'g')
  g.setAttribute('transform', `rotate(${deg})`)
  const r = document.createElementNS(SVG, 'rect')
  r.setAttribute('x', '-7'); r.setAttribute('y', '-77'); r.setAttribute('width', '14'); r.setAttribute('height', '18'); r.setAttribute('rx', '1.5')
  r.setAttribute('style', `--d:${rnd(2.2, 5.5).toFixed(2)}s;--o:${rnd(0, 3).toFixed(2)}s`)
  g.appendChild(r)
  for (let k = 0; k < 4; k++) { // copper windings
    const w = document.createElementNS(SVG, 'path')
    w.setAttribute('class', 'wire'); w.setAttribute('d', `M-6 ${-74 + k * 4} L6 ${-73 + k * 4}`)
    g.appendChild(w)
  }
  coils.appendChild(g)
}
const bolts = arc.querySelector('.bolts')!
const zap = () => { // a jagged arc between two random points on the inner ring
  bolts.innerHTML = ''
  for (let n = 0; n < 3; n++) {
    const a0 = rnd(0, Math.PI * 2), a1 = a0 + rnd(.4, 1.2), steps = 7
    let d = ''
    for (let s = 0; s <= steps; s++) {
      const a = a0 + (a1 - a0) * (s / steps), r = 52 + (s && s < steps ? rnd(-9, 9) : 0)
      d += `${s ? 'L' : 'M'}${(Math.cos(a) * r).toFixed(1)} ${(Math.sin(a) * r).toFixed(1)} `
    }
    const p = document.createElementNS(SVG, 'path')
    p.setAttribute('d', d)
    bolts.appendChild(p)
  }
}

// ---------------------------------------------------------------- the app's reactor
const canvas = $<HTMLCanvasElement>('#reactor')
const reactor = new Reactor(canvas)
reactor.fitTo($('#core'))
if (reduced) reactor.setCalm(true)
const fpsEl = $('#fps')
let cap = 30
const setCap = (c: number) => { if (c === cap) return; cap = c; reactor.setFrameCap(c); fpsEl.textContent = String(c) }
document.addEventListener('visibilitychange', () => setCap(document.hidden ? 0 : scrollY > innerHeight * .6 ? 12 : 30))

// dim the reactor and the HUD core once the hero scrolls away (the app idles at 12 fps when blurred, too)
const core = $('#core')
const onScroll = () => {
  const p = Math.min(1, scrollY / (innerHeight * .8))
  core.style.opacity = String(1 - p * .82)
  canvas.style.opacity = String(1 - p * .55)
  if (!document.hidden) setCap(p > .75 ? 12 : 30)
}
addEventListener('scroll', onScroll, { passive: true })
onScroll()

// ---------------------------------------------------------------- toasts
const toast = $('#toast')
let toastT = 0
const say = (msg: string, bad = false) => {
  toast.textContent = msg; toast.classList.toggle('bad', bad); toast.classList.add('on')
  clearTimeout(toastT); toastT = window.setTimeout(() => toast.classList.remove('on'), 2600)
}

// ---------------------------------------------------------------- power surges (random flicker + reactor signals)
const title = $('#title')
const surge = () => {
  if (reduced) return
  zap()
  core.classList.remove('surge'); void core.offsetWidth; core.classList.add('surge')
  title.classList.remove('on'); void title.offsetWidth; title.classList.add('on')
  reactor.pulse(10)
}
const scheduleSurge = () => window.setTimeout(() => { if (!document.hidden) surge(); scheduleSurge() }, rnd(6500, 14000))
if (!reduced) scheduleSurge()

// ---------------------------------------------------------------- demo controls
const routineBar = $('#w-routine')
let starting = false
async function startMyDay() {
  if (starting) return
  starting = true
  reactor.burst()
  surge()
  say('Start my day: opening 5 apps across 3 monitors (demo)')
  const t0 = performance.now(), dur = reduced ? 10 : 3200
  await new Promise<void>((done) => {
    const step = (now: number) => {
      const f = Math.min(1, (now - t0) / dur)
      reactor.setProgress(f); routineBar.style.width = `${f * 100}%`
      if (Math.floor(f * 5) !== Math.floor(Math.max(0, f - .02) * 5)) reactor.pulse(6)
      if (f < 1) requestAnimationFrame(step); else done()
    }
    requestAnimationFrame(step)
  })
  window.setTimeout(() => { reactor.setProgress(null); say('All windows placed. Good morning.'); starting = false }, 400)
}
$('#start').addEventListener('click', startMyDay)
for (const b of $$('[data-act]')) {
  b.addEventListener('click', () => {
    const act = b.dataset.act
    if (act === 'start') startMyDay()
    else if (act === 'pulse') { reactor.pulse(12); surge(); say('Launching… signals fired along the lattice') }
    else if (act === 'fail') {
      reactor.fail(); core.classList.add('fault'); $('#state').textContent = 'fault'; $('#state').className = 'bad'
      say('Launch failed: target not found (demo)', true)
      window.setTimeout(() => { core.classList.remove('fault'); $('#state').textContent = 'nominal'; $('#state').className = 'ok' }, 1300)
    } else if (act === 'wake') { reactor.wake(); say('Reactor restarting…') }
    else if (act === 'calm') {
      const on = b.getAttribute('aria-pressed') !== 'true'
      b.setAttribute('aria-pressed', String(on)); reactor.setCalm(on); say(on ? 'Calm motion: a still glow' : 'Full motion')
    }
  })
}

// ---------------------------------------------------------------- themes (the app's own presets and math)
const themesEl = $('.themes')
const applyTheme = (name: keyof typeof THEMES) => {
  const vars = themeVars({ theme: name, accent: undefined, glass: 45 } as Parameters<typeof themeVars>[0])
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(k, v)
  $('#theme-name').textContent = THEMES[name].label
  for (const b of $$('.themes button')) b.setAttribute('aria-checked', String(b.dataset.theme === name))
}
for (const [name, p] of Object.entries(THEMES)) {
  const b = document.createElement('button')
  b.setAttribute('role', 'radio'); b.dataset.theme = name
  b.innerHTML = `<span class="sw"><i style="background:${p.sea}"></i><i style="background:${p.line}"></i><i style="background:${p.amber}"></i></span><span>${p.label}<br><small>${p.blurb}</small></span>`
  b.addEventListener('click', () => { applyTheme(name as keyof typeof THEMES); reactor.pulse(6) })
  themesEl.appendChild(b)
}
applyTheme('command')

// ---------------------------------------------------------------- mock tiles and widgets
const TILES: [string, string][] = [['Mail', 'link'], ['Calendar', 'link'], ['Notes', 'app'], ['Terminal', 'app'], ['Projects', 'folder'], ['Build & test', 'script']]
const tilesEl = $('#tiles')
const recent: string[] = []
for (const [name, kind] of TILES) {
  const b = document.createElement('button')
  b.innerHTML = `<b>${name}</b><small>${kind}</small>`
  b.addEventListener('click', () => {
    b.classList.remove('go'); void b.offsetWidth; b.classList.add('go')
    reactor.pulse(8)
    recent.unshift(name); recent.length = Math.min(recent.length, 3)
    $('#w-recent').textContent = recent.join(' · ')
    say(`Launching ${name}… (demo: nothing actually opens)`)
  })
  tilesEl.appendChild(b)
}
window.setInterval(() => { $('#w-cpu').textContent = String(Math.round(rnd(6, 24))); $('#w-ram').textContent = String(Math.round(rnd(38, 46))) }, 2000)

// ---------------------------------------------------------------- clock, readouts, uptime
const clock = $('#clock'), secs = $('#secs'), date = $('#date')
const pad = (n: number) => String(n).padStart(2, '0')
const t0 = Date.now()
const tick = () => {
  const d = new Date()
  clock.textContent = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  secs.textContent = pad(d.getSeconds())
  date.textContent = d.toLocaleDateString(undefined, { weekday: 'long', month: 'short', day: 'numeric' })
  const up = Math.floor((Date.now() - t0) / 1000)
  $('#uptime').textContent = `${pad(Math.floor(up / 60))}:${pad(up % 60)}`
  $('#out').textContent = (3 + rnd(-.04, .04)).toFixed(2)
  $('#temp').textContent = String(Math.round(312 + rnd(-3, 3)))
}
tick(); window.setInterval(tick, 1000)

// ---------------------------------------------------------------- panels reveal, copy buttons
if ('IntersectionObserver' in window && !reduced) {
  const io = new IntersectionObserver((es) => { for (const e of es) if (e.isIntersecting) { e.target.classList.add('shown'); io.unobserve(e.target) } }, { threshold: .12 })
  for (const p of $$('.panel')) { p.classList.add('reveal'); io.observe(p) }
}
for (const b of $$('.copy')) {
  b.addEventListener('click', async () => {
    const text = (b.previousElementSibling as HTMLElement).textContent ?? ''
    try { await navigator.clipboard.writeText(text) } catch { /* clipboard blocked: the text is still selectable */ }
    b.textContent = 'Copied'; b.classList.add('done')
    window.setTimeout(() => { b.textContent = 'Copy'; b.classList.remove('done') }, 1600)
  })
}

// ---------------------------------------------------------------- boot sequence (once per visit)
const boot = $('#boot')
const LINES: [string, string][] = [
  ['> bridge core', 'loaded'], ['> arc reactor', 'online'], ['> neural lattice', '152 nodes linked'],
  ['> displays', 'detected'], ['> tiles and workspaces', 'ready'], ['> network', 'none (local only)'],
]
const finishBoot = () => { boot.classList.add('done'); try { sessionStorage.setItem('bridge.booted', '1') } catch { /* */ } window.setTimeout(() => boot.remove(), 800); reactor.wake() }
let booted = false
try { booted = sessionStorage.getItem('bridge.booted') === '1' } catch { /* */ }
if (reduced || booted) { boot.remove() }
else {
  const log = $('#boot-log'), fill = $('#boot-fill')
  let i = 0, skipped = false
  const skip = () => { if (skipped) return; skipped = true; finishBoot() }
  addEventListener('keydown', skip, { once: true }); boot.addEventListener('click', skip)
  const next = () => {
    if (skipped) return
    if (i < LINES.length) {
      const [k, v] = LINES[i++]
      log.innerHTML += `${k.padEnd(24, '.')} <span class="ok">${v}</span>\n`
      fill.style.width = `${(i / (LINES.length + 1)) * 100}%`
      window.setTimeout(next, rnd(160, 320))
    } else {
      log.innerHTML += `\n<span class="hi">Good ${new Date().getHours() < 12 ? 'morning' : new Date().getHours() < 18 ? 'afternoon' : 'evening'}. All systems online.</span>`
      fill.style.width = '100%'
      window.setTimeout(skip, 900)
    }
  }
  window.setTimeout(next, 350)
}
