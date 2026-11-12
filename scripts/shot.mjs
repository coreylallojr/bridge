// Screenshots of the built Tauri app at the user's 3 monitor sizes, over WebView2's DevTools protocol.
// Usage: npm run tauri:build && node scripts/shot.mjs [--only 1920x1080] [--out out/shots] [--wait 2500] [--profile none]
// Always runs against a throwaway BRIDGE_HOME with dry launches.
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { launchBridge } from '../tests/e2e/launch.mjs'

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d }
const sizes = arg('--only') ? [arg('--only')] : ['1920x1080', '1080x1920', '1280x720']
const outDir = resolve(arg('--out', 'out/shots'))
const waitMs = Number(arg('--wait', 2500))
const seed = arg('--profile', 'tests/fixtures/demo-profile.json')
mkdirSync(outDir, { recursive: true })

for (const size of sizes) {
  const home = mkdtempSync(join(tmpdir(), 'bridge-shot-'))
  if (seed !== 'none' && existsSync(seed)) copyFileSync(seed, join(home, 'profile.json'))
  const app = await launchBridge({ home, size })
  try {
    await app.page.waitForTimeout(waitMs)
    const file = join(outDir, `home-${size}.png`)
    await app.page.screenshot({ path: file })
    console.log(file)
  } finally {
    await app.close()
    try { rmSync(home, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }) } catch { /* WebView2 may still hold files; temp dir */ }
  }
}
