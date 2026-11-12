// Draws Bridge's icon (cyan reactor ring + bright core) and writes resources/icon.png (256) and resources/tray.png (32).
// Pure Node: supersampled shapes + a hand-rolled PNG encoder (zlib only). Run: node scripts/make-icons.mjs
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { deflateSync } from 'node:zlib'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const LINE = [0x3f, 0xe0, 0xee] // #3FE0EE
const CORE = [0xe8, 0xfd, 0xff] // near-white cyan

// ---- PNG encoding ----
const CRC_TABLE = new Uint32Array(256).map((_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})
function crc32(buf) {
  let c = 0xffffffff
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}
function chunk(type, data) {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(td))
  return Buffer.concat([len, td, crc])
}
function encodePng(size, rgba) {
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(size, 0)
  ihdr.writeUInt32BE(size, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // RGBA
  const raw = Buffer.alloc(size * (size * 4 + 1))
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0 // filter: none
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4)
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

// ---- drawing ----
/** Composites `color` at `alpha` over the premultiplied pixel. */
function over(px, color, alpha) {
  if (alpha <= 0) return
  const a = Math.min(1, alpha)
  for (let i = 0; i < 3; i++) px[i] = (color[i] / 255) * a + px[i] * (1 - a)
  px[3] = a + px[3] * (1 - a)
}

function draw(size) {
  const c = size / 2
  // Small icons get relatively thicker strokes so the ring stays readable in the tray.
  const small = size <= 48
  const ringR = size * (small ? 0.36 : 0.37)
  const ringHalf = size * (small ? 0.065 : 0.045)
  const coreR = size * (small ? 0.14 : 0.12)
  const glowR = size * 0.5
  const SS = 5 // supersamples per axis
  const out = Buffer.alloc(size * size * 4)
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let ring = 0
      let core = 0
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const d = Math.hypot(x + (sx + 0.5) / SS - c, y + (sy + 0.5) / SS - c)
          if (Math.abs(d - ringR) <= ringHalf) ring++
          if (d <= coreR) core++
        }
      }
      ring /= SS * SS
      core /= SS * SS
      const d = Math.hypot(x + 0.5 - c, y + 0.5 - c)
      const px = [0, 0, 0, 0] // premultiplied rgb + alpha
      // Soft glow behind the ring and around the core (kept subtle on transparent backgrounds).
      const ringGlow = Math.exp(-(((d - ringR) / (size * 0.07)) ** 2)) * (small ? 0.18 : 0.32)
      const coreGlow = Math.exp(-((d / (size * 0.2)) ** 2)) * 0.55
      if (d < glowR) over(px, LINE, ringGlow * (1 - d / glowR) * 2)
      over(px, LINE, coreGlow)
      over(px, LINE, ring)
      over(px, CORE, core)
      const i = (y * size + x) * 4
      const a = px[3]
      out[i] = a > 0 ? Math.round((px[0] / a) * 255) : 0
      out[i + 1] = a > 0 ? Math.round((px[1] / a) * 255) : 0
      out[i + 2] = a > 0 ? Math.round((px[2] / a) * 255) : 0
      out[i + 3] = Math.round(a * 255)
    }
  }
  return encodePng(size, out)
}

mkdirSync(join(root, 'resources'), { recursive: true })
for (const [file, size] of [['icon.png', 256], ['tray.png', 32]]) {
  writeFileSync(join(root, 'resources', file), draw(size))
  console.log(`wrote resources/${file} (${size}×${size})`)
}
