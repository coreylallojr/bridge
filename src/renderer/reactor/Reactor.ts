// The live reactor behind the clock. A port of spike/d-reactor.html (itself ported from ARC_Reactor's
// jarvis-neural.html) minus audio: voice arrives in milestone 5 and plugs into `audio`.
// Rules (CLAUDE.md): bloom on pure black (sea tint is a CSS screen-blend layer), sized to its box, calm = still glow.
import * as THREE from 'three'
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js'
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js'
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js'
import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js'
import { FOCUSED_FPS, maxStep, RENDER_BUDGET_PX, renderRatio, shouldDraw } from './frameGate'

const OUTER_N = 110, INNER_N = 42, N = OUTER_N + INNER_N, OUTER_R = 68, INNER_R = 36
const PHI = Math.PI * (3 - Math.sqrt(5))
const MAX_SIG = 40
const WAKE = 2.0, BURST = 1.3, FAIL = 1.2
const backOut = (p: number) => { const c = 1.9; return 1 + (c + 1) * Math.pow(p - 1, 3) + c * Math.pow(p - 1, 2) }

// [radius, tube, color, rotX, rotZ, spin]
const RINGS: [number, number, number, number, number, number][] = [
  [83, .75, 0x1aa6d9, Math.PI / 2, 0, .00085], [80, .55, 0x3fe0ee, Math.PI / 3, Math.PI / 4, -.0012],
  [77, .5, 0x0d7fb8, Math.PI * .17, -Math.PI / 3, .0016], [97, .3, 0x0b4f78, .18, .55, -.00045],
  [62, .9, 0x5cecf7, Math.PI / 2 * .72, Math.PI / 2, .0021], [88, .35, 0x0e6a99, Math.PI * .42, Math.PI * .28, -.0008],
]

interface Signal { e: number; t: number; spd: number; rev: boolean }

interface Gl {
  renderer: THREE.WebGLRenderer
  composer: EffectComposer
  bloom: UnrealBloomPass
  scene: THREE.Scene
  camera: THREE.PerspectiveCamera
  net: THREE.Group
  nodeGeo: THREE.BufferGeometry
  nodeMat: THREE.ShaderMaterial
  edgeGeo: THREE.BufferGeometry
  edgeMat: THREE.LineBasicMaterial
  sigGeo: THREE.BufferGeometry
  coreMesh: THREE.Mesh
  rings: { m: THREE.Mesh<THREE.TorusGeometry, THREE.MeshBasicMaterial>; spd: number }[]
  procMesh: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>
  errMat: THREE.MeshBasicMaterial
  tex: THREE.CanvasTexture
}

export class Reactor {
  private gl: Gl | null = null
  private canvas: HTMLCanvasElement
  // Geometry state (CPU side).
  private base = new Float32Array(N * 3)
  private dir = new Float32Array(N * 3)
  private nodeBuf = new Float32Array(N * 3)
  private sizeBuf = new Float32Array(N)
  private bPh = new Float32Array(N)
  private bFq = new Float32Array(N)
  private bBase = new Float32Array(N)
  private flash = new Float32Array(N)
  private edges: [number, number][] = []
  private edgeBuf = new Float32Array(0)
  private sigBuf = new Float32Array(MAX_SIG * 3)
  private signals: Signal[] = []
  // Animation state.
  private wakeT = 0
  private burstT = -1
  private errT = 0
  private procTarget = 0
  private calm = false
  private t = 0
  /** 0–1 voice level. Milestone 5 drives this from the Web Audio analyser. */
  private audio = 0
  // Loop state.
  /** Max frames per second; 0 = paused (no loop). See frameGate.ts. */
  private cap = FOCUSED_FPS
  private budget = RENDER_BUDGET_PX
  private raf = 0
  private last = 0
  private lastDraw = 0
  private disposed = false
  private box: HTMLElement | null = null
  private ro: ResizeObserver | null = null
  private onResize = () => this.fit()

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas
    this.buildNodes()
    try {
      this.gl = this.buildGl()
    } catch (err) {
      // No WebGL (driver blocklist, remote session…): leave the canvas black; the UI still works.
      console.warn('Reactor disabled: WebGL unavailable', err)
      this.gl = null
    }
    this.kick()
  }

  /** Center the reactor on `el`'s box and size it to it; follows resizes. */
  fitTo(el: HTMLElement): void {
    this.box = el
    this.ro?.disconnect()
    this.ro = new ResizeObserver(this.onResize)
    this.ro.observe(el)
    removeEventListener('resize', this.onResize)
    addEventListener('resize', this.onResize)
    this.fit()
  }

  /** Restart the grow-in animation. */
  wake(): void {
    this.wakeT = this.calm ? WAKE : 0
    this.kick()
  }

  /** Converge, then explode outward and settle (Start my day). */
  burst(): void {
    if (this.calm) return
    this.burstT = 0
    this.fire(18)
    this.kick()
  }

  /** Signals along the edges plus node flashes (each launch). */
  pulse(n = 8): void {
    if (this.calm) return
    this.fire(n)
    for (let i = 0; i < n + 2; i++) this.flash[(Math.random() * N) | 0] = 1
    this.kick()
  }

  /** Red error ring + red nodes for ~1.2 s. Shown in calm mode too. */
  fail(): void {
    this.errT = FAIL
    this.kick()
  }

  /** Processing ring: 0–1 fills clockwise from the top; null hides it. */
  setProgress(f: number | null): void {
    if (f === null) this.procTarget = 0
    else {
      this.procTarget = 1
      const gl = this.gl
      if (gl) {
        gl.procMesh.geometry.dispose()
        gl.procMesh.geometry = new THREE.RingGeometry(104, 108, 96, 1, Math.PI / 2, -Math.max(.001, Math.min(1, f)) * Math.PI * 2)
      }
    }
    this.kick()
  }

  /** Calm motion / reduced motion: a still glow, rendered only when something changes. */
  setCalm(on: boolean): void {
    if (on === this.calm) return
    this.calm = on
    if (on) {
      this.wakeT = WAKE
      this.burstT = -1
      this.signals.length = 0
      this.flash.fill(0)
    }
    this.kick()
  }

  /**
   * Draw at most `fps` frames per second (30 focused, 12 blurred: see frameGate.capFor). 0 pauses the loop entirely
   * (hidden); raising it again resumes without a time jump.
   */
  setFrameCap(fps: number): void {
    const cap = fps > 0 ? fps : 0
    if (cap === this.cap) return
    this.cap = cap
    if (cap === 0) this.stop()
    else this.kick()
  }

  /** Internal render budget in pixels (quality setting); re-fits immediately. */
  setRenderBudget(px: number): void {
    if (px === this.budget) return
    this.budget = px
    this.fit()
    this.kick()
  }

  dispose(): void {
    this.disposed = true
    this.stop()
    this.ro?.disconnect()
    this.ro = null
    removeEventListener('resize', this.onResize)
    const gl = this.gl
    this.gl = null
    if (!gl) return
    gl.scene.traverse((o) => {
      const m = o as THREE.Mesh
      if (m.geometry) m.geometry.dispose()
      const mat = m.material as THREE.Material | THREE.Material[] | undefined
      if (Array.isArray(mat)) mat.forEach((x) => x.dispose())
      else mat?.dispose()
    })
    gl.tex.dispose()
    gl.composer.passes.forEach((p) => p.dispose())
    gl.composer.dispose()
    gl.renderer.dispose()
  }

  // ── internals ────────────────────────────────────────────────────────────────

  private buildNodes(): void {
    const { base, dir } = this
    const place = (start: number, count: number, R: number) => {
      for (let i = 0; i < count; i++) {
        const y = 1 - (i / (count - 1)) * 2, rad = Math.sqrt(Math.max(0, 1 - y * y)), th = PHI * i, k = (start + i) * 3
        base[k] = Math.cos(th) * rad * R; base[k + 1] = y * R; base[k + 2] = Math.sin(th) * rad * R
        const len = Math.hypot(base[k], base[k + 1], base[k + 2])
        dir[k] = base[k] / len; dir[k + 1] = base[k + 1] / len; dir[k + 2] = base[k + 2] / len
      }
    }
    place(0, OUTER_N, OUTER_R)
    place(OUTER_N, INNER_N, INNER_R)
    for (let i = 0; i < N; i++) {
      this.bPh[i] = Math.random() * 6.28; this.bFq[i] = .25 + Math.random() * .45; this.bBase[i] = 1.4 + Math.random() * 2.2
    }
    for (let i = 0; i < N; i++) {
      const near: [number, number][] = []
      for (let j = i + 1; j < N; j++) {
        const dx = base[i * 3] - base[j * 3], dy = base[i * 3 + 1] - base[j * 3 + 1], dz = base[i * 3 + 2] - base[j * 3 + 2]
        const d = dx * dx + dy * dy + dz * dz
        if (d < 42 * 42) near.push([j, d])
      }
      near.sort((a, b) => a[1] - b[1])
      for (let k = 0; k < Math.min(5, near.length); k++) this.edges.push([i, near[k][0]])
    }
    this.edgeBuf = new Float32Array(this.edges.length * 6)
    this.nodeBuf.set(base)
  }

  private buildGl(): Gl {
    // No MSAA on the canvas: the scene is drawn into the composer's (non-multisampled) targets and only OutputPass's
    // full-screen quad reaches the canvas, so canvas antialiasing cost GPU memory/bandwidth for no visible change.
    const renderer = new THREE.WebGLRenderer({ canvas: this.canvas, antialias: false })
    renderer.setPixelRatio(renderRatio(innerWidth, innerHeight, devicePixelRatio))
    renderer.setClearColor(0x000000, 1) // bloom must run on pure black; the sea tint is a CSS screen-blend layer
    renderer.toneMapping = THREE.ReinhardToneMapping
    renderer.toneMappingExposure = 1.5
    renderer.setSize(innerWidth, innerHeight, false)
    const scene = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(55, innerWidth / innerHeight, 0.1, 2000)
    camera.position.set(0, 0, 370)
    camera.lookAt(0, 0, 0)
    const composer = new EffectComposer(renderer)
    composer.addPass(new RenderPass(scene, camera))
    const bloom = new UnrealBloomPass(new THREE.Vector2(innerWidth, innerHeight), 1.6, 0.6, 0.0)
    composer.addPass(bloom)
    composer.addPass(new OutputPass())

    const net = new THREE.Group()
    scene.add(net)

    const edgeGeo = new THREE.BufferGeometry()
    edgeGeo.setAttribute('position', new THREE.BufferAttribute(this.edgeBuf, 3))
    const edgeMat = new THREE.LineBasicMaterial({ color: 0x0a7fa8, transparent: true, opacity: .5 })
    net.add(new THREE.LineSegments(edgeGeo, edgeMat))

    const spr = document.createElement('canvas')
    spr.width = spr.height = 64
    const sg = spr.getContext('2d')!
    const gr = sg.createRadialGradient(32, 32, 0, 32, 32, 32)
    gr.addColorStop(0, 'rgba(255,255,255,1)'); gr.addColorStop(.12, 'rgba(210,250,255,.95)')
    gr.addColorStop(.3, 'rgba(63,224,238,.75)'); gr.addColorStop(.58, 'rgba(0,120,160,.3)'); gr.addColorStop(1, 'rgba(0,0,0,0)')
    sg.fillStyle = gr
    sg.fillRect(0, 0, 64, 64)
    const tex = new THREE.CanvasTexture(spr)

    const nodeGeo = new THREE.BufferGeometry()
    nodeGeo.setAttribute('position', new THREE.BufferAttribute(this.nodeBuf, 3))
    nodeGeo.setAttribute('size', new THREE.BufferAttribute(this.sizeBuf, 1))
    const nodeMat = new THREE.ShaderMaterial({
      uniforms: { tex: { value: tex }, speak: { value: 0 }, err: { value: 0 } },
      vertexShader: `attribute float size; uniform float speak; varying float vS;
        void main(){ vS = speak; vec4 mv = modelViewMatrix * vec4(position,1.0); gl_PointSize = size * (300.0 / -mv.z); gl_Position = projectionMatrix * mv; }`,
      fragmentShader: `uniform sampler2D tex; uniform float err; varying float vS;
        void main(){ vec4 s = texture2D(tex, gl_PointCoord); if (s.a < .04) discard;
          vec3 c = mix(vec3(.25,.88,.93), vec3(.85,.98,1.0), vS); c = mix(c, vec3(1.0,.2,.18), err*.7); gl_FragColor = vec4(c, s.a); }`,
      transparent: true, depthWrite: false, blending: THREE.AdditiveBlending,
    })
    net.add(new THREE.Points(nodeGeo, nodeMat))
    const coreMesh = new THREE.Mesh(new THREE.SphereGeometry(4.5, 20, 20), new THREE.MeshBasicMaterial({ color: 0xffffff }))
    net.add(coreMesh)

    const sigGeo = new THREE.BufferGeometry()
    sigGeo.setAttribute('position', new THREE.BufferAttribute(this.sigBuf, 3))
    sigGeo.setDrawRange(0, 0)
    net.add(new THREE.Points(sigGeo, new THREE.PointsMaterial({
      color: 0x7ff3ff, size: 5, map: tex, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false,
    })))

    const rings = RINGS.map(([r, tube, col, rx, rz, spd]) => {
      const m = new THREE.Mesh(new THREE.TorusGeometry(r, tube, 6, 120), new THREE.MeshBasicMaterial({ color: col, transparent: true, opacity: 0 }))
      m.rotation.x = rx; m.rotation.z = rz
      scene.add(m)
      return { m, spd }
    })
    const procMesh = new THREE.Mesh(
      new THREE.RingGeometry(104, 108, 96, 1, Math.PI / 2, 0.001),
      new THREE.MeshBasicMaterial({ color: 0x3fe0ee, transparent: true, opacity: 0, side: THREE.DoubleSide }),
    )
    scene.add(procMesh)
    const errMat = new THREE.MeshBasicMaterial({ color: 0xff3030, transparent: true, opacity: 0, side: THREE.DoubleSide })
    scene.add(new THREE.Mesh(new THREE.RingGeometry(72, 77, 96), errMat))

    return { renderer, composer, bloom, scene, camera, net, nodeGeo, nodeMat, edgeGeo, edgeMat, sigGeo, coreMesh, rings, procMesh, errMat, tex }
  }

  private fit(): void {
    const gl = this.gl
    if (!gl || this.disposed) return
    const w = innerWidth, h = innerHeight
    // Follows window size and monitor DPI (dragging to another display changes devicePixelRatio).
    const ratio = renderRatio(w, h, devicePixelRatio, this.budget)
    if (ratio !== gl.renderer.getPixelRatio()) {
      gl.renderer.setPixelRatio(ratio)
      gl.composer.setPixelRatio(ratio)
    }
    gl.renderer.setSize(w, h, false)
    gl.composer.setSize(w, h)
    const r = this.box?.getBoundingClientRect()
    if (r && r.width > 0 && r.height > 0) {
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2 // same point as the clock (.readout)
      // outermost ring (~100 units) spans 46% of the box's short side
      const pxPerUnit = 0.46 * Math.min(r.width, r.height) / 100
      gl.camera.position.z = (h / pxPerUnit) / (2 * Math.tan(THREE.MathUtils.degToRad(gl.camera.fov / 2)))
      gl.camera.setViewOffset(w, h, w / 2 - cx, h / 2 - cy, w, h)
    }
    gl.camera.aspect = w / h
    gl.camera.updateProjectionMatrix()
    this.kick()
  }

  private fire(n: number): void {
    for (let i = 0; i < n && this.signals.length < MAX_SIG; i++) {
      this.signals.push({ e: (Math.random() * this.edges.length) | 0, t: 0, spd: .5 + Math.random() * .7, rev: Math.random() < .5 })
    }
  }

  /** Schedule a frame. Coming back from a stopped loop resets the clock so there is no dt jump. */
  private kick(): void {
    if (this.raf || this.disposed || !this.gl || this.cap === 0) return
    this.last = performance.now()
    this.raf = requestAnimationFrame(this.frame)
  }

  private stop(): void {
    if (this.raf) cancelAnimationFrame(this.raf)
    this.raf = 0
  }

  /** In calm mode only fades (fail ring, progress ring) keep the loop alive. */
  private animating(): boolean {
    if (!this.calm) return true
    const gl = this.gl
    if (!gl) return false
    return this.errT > 0 || gl.errMat.opacity > .002 || Math.abs(gl.procMesh.material.opacity - (this.procTarget > 0 ? .85 : 0)) > .005
  }

  private frame = (now: number): void => {
    this.raf = 0
    if (this.disposed || !this.gl || this.cap === 0) return
    // Over budget: skip this vsync (no step, no render). `last` keeps the previous draw, so dt spans the skipped frames.
    if (!shouldDraw(now, this.lastDraw, this.cap)) {
      this.raf = requestAnimationFrame(this.frame)
      return
    }
    const dt = Math.max(0, Math.min(maxStep(this.cap), (now - this.last) / 1000))
    this.last = now
    this.lastDraw = now
    this.step(dt)
    this.gl.composer.render()
    if (this.animating()) this.raf = requestAnimationFrame(this.frame)
  }

  private step(dt: number): void {
    const gl = this.gl!
    const { calm, base, dir, nodeBuf, sizeBuf, flash, edges, edgeBuf, sigBuf, signals } = this
    const audio = calm ? 0 : this.audio
    const speed = calm ? 0 : 1
    this.t += dt * speed

    // wake: grow from nothing, rings fade in
    if (this.wakeT < WAKE) this.wakeT += dt
    const wp = Math.min(1, this.wakeT / WAKE)
    let scale = backOut(wp)
    // burst: converge, then explode outward and settle
    if (this.burstT >= 0) {
      this.burstT += dt
      const p = Math.min(1, this.burstT / BURST)
      scale *= p < .3 ? 1 - .7 * (p / .3) : .3 + .7 * backOut((p - .3) / .7)
      if (p >= 1) this.burstT = -1
    }
    gl.net.scale.setScalar(Math.max(.01, scale))
    gl.net.rotation.y += dt * .05 * speed * (1 + audio * 2)

    for (let i = 0; i < N; i++) {
      const k = i * 3, br = calm ? 0 : Math.sin(this.t * this.bFq[i] * 2 + this.bPh[i]) * 1.6 + audio * 6
      nodeBuf[k] = base[k] + dir[k] * br; nodeBuf[k + 1] = base[k + 1] + dir[k + 1] * br; nodeBuf[k + 2] = base[k + 2] + dir[k + 2] * br
      flash[i] = Math.max(0, flash[i] - dt * 1.8)
      sizeBuf[i] = this.bBase[i] * (1 + audio * 1.4 + flash[i] * 2.5)
    }
    gl.nodeGeo.attributes.position.needsUpdate = true
    gl.nodeGeo.attributes.size.needsUpdate = true
    for (let e = 0; e < edges.length; e++) {
      const [a, b] = edges[e]
      edgeBuf.set(nodeBuf.subarray(a * 3, a * 3 + 3), e * 6)
      edgeBuf.set(nodeBuf.subarray(b * 3, b * 3 + 3), e * 6 + 3)
    }
    gl.edgeGeo.attributes.position.needsUpdate = true
    gl.edgeMat.opacity = .35 + audio * .4

    // signals travel along edges
    if (!calm && Math.random() < dt * 2.2) this.fire(1)
    for (let s = signals.length - 1; s >= 0; s--) {
      const g = signals[s]
      g.t += dt * g.spd * 1.6
      if (g.t >= 1) { flash[edges[g.e][g.rev ? 0 : 1]] = 1; signals.splice(s, 1) }
    }
    sigBuf.fill(0)
    signals.forEach((g, i) => {
      const [a, b] = g.rev ? [edges[g.e][1], edges[g.e][0]] : edges[g.e]
      for (let c = 0; c < 3; c++) sigBuf[i * 3 + c] = nodeBuf[a * 3 + c] + (nodeBuf[b * 3 + c] - nodeBuf[a * 3 + c]) * g.t
    })
    gl.sigGeo.setDrawRange(0, signals.length)
    gl.sigGeo.attributes.position.needsUpdate = true

    const rm = (1 + audio * 5) * speed
    gl.rings.forEach(({ m, spd }, i) => {
      m.rotation.y += spd * rm * 60 * dt
      m.material.opacity = Math.min(.8, Math.max(0, wp * 1.4 - i * .08))
    })
    gl.coreMesh.scale.setScalar(calm ? 1 : 1 + .12 * Math.sin(this.t * 1.05) + audio * 1.6)
    const pm = gl.procMesh.material
    pm.opacity += ((this.procTarget > 0 ? .85 : 0) - pm.opacity) * Math.min(1, dt * 4)
    this.errT = Math.max(0, this.errT - dt)
    gl.errMat.opacity = this.errT * .9
    gl.nodeMat.uniforms.err.value = this.errT
    gl.nodeMat.uniforms.speak.value = audio
    gl.bloom.strength = (calm ? 1.1 : 1.6) + audio * 1.6 + (this.burstT >= 0 ? 1.2 : 0)
  }
}
