// End-to-end checks, driving the BUILT Tauri app over WebView2's DevTools protocol.
// Run: npm run tauri:build && npm run test:e2e
// Safe by construction: temp BRIDGE_HOME, BRIDGE_DRY_LAUNCH=1 (nothing really opens), a local fixture web page,
// and the real clipboard is saved and restored.
import { test, before, after } from 'node:test'
import assert from 'node:assert/strict'
import { copyFileSync, existsSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { execFileSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchBridge, clipboard, repo } from './launch.mjs'

let home, app, page, server, fixtureUrl, savedClipboard

async function launch() {
  app = await launchBridge({ home })
  page = app.page
}

const launches = () =>
  existsSync(join(home, 'launch-log.jsonl'))
    ? readFileSync(join(home, 'launch-log.jsonl'), 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
    : []

/** Waits for a toast whose text matches (old toasts may still be on screen). */
const toast = (re) => page.locator('.toast .msg', { hasText: re }).first().waitFor({ timeout: 8000 })

/** Simulates Edge/Explorer dropping data on the window (same DataTransfer shapes Electron delivers). */
async function drop(type, value) {
  await page.evaluate(([t, v]) => {
    const dt = new DataTransfer()
    dt.setData(t, v)
    for (const name of ['dragenter', 'dragover', 'drop']) {
      document.body.dispatchEvent(new DragEvent(name, { dataTransfer: dt, bubbles: true, cancelable: true }))
    }
  }, [type, value])
}

before(async () => {
  home = mkdtempSync(join(tmpdir(), 'bridge-e2e-'))
  copyFileSync(join(repo, 'tests/fixtures/demo-profile.json'), join(home, 'profile.json'))
  server = createServer((q, r) => {
    if (q.url === '/go') return r.writeHead(302, { location: '/landed' }).end()
    if (q.url === '/landed') return r.writeHead(200, { 'content-type': 'text/html' }).end('<title>Redirect landed</title>')
    r.writeHead(200, { 'content-type': 'text/html' }).end('<title>Local fixture page</title>')
  })
  await new Promise((res) => server.listen(0, '127.0.0.1', res))
  fixtureUrl = `http://127.0.0.1:${server.address().port}/page`
  await launch()
  savedClipboard = clipboard.read()
})

after(async () => {
  try { clipboard.write(savedClipboard ?? '') } catch { /* best effort */ }
  await app?.close().catch(() => {})
  server?.close()
  try { rmSync(home, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }) } catch { /* temp */ }
})

test('home renders with groups, tiles, reactor clock and Start my day', async () => {
  assert.equal(await page.locator('nav').getByText('Work').count() > 0, true)
  assert.ok((await page.locator('.tile').count()) >= 4)
  assert.match((await page.locator('.readout').textContent()) ?? '', /\d{1,2}:\d{2}/)
  assert.equal(await page.locator('#start-my-day').isEnabled(), true)
})

test('cold start: window ready in under 2 s', () => {
  const ms = app.readyMs()
  assert.ok(ms.length > 0, 'no timing line in bridge.log')
  assert.ok(ms[0] < 2000, `ready took ${ms[0]} ms`)
})

test('dropping a file from Explorer adds it by its real path', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'bridge-drop-'))
  writeFileSync(join(dir, 'dropped.txt'), 'hi')
  const cdp = await page.context().newCDPSession(page)
  const data = { items: [], files: [join(dir, 'dropped.txt').replaceAll('\\', '/')], dragOperationsMask: 1 }
  for (const type of ['dragEnter', 'dragOver', 'drop']) await cdp.send('Input.dispatchDragEvent', { type, x: 700, y: 450, data })
  await toast(/Added dropped/)
  const p = JSON.parse(readFileSync(join(home, 'profile.json'), 'utf8'))
  assert.equal(realpathSync.native(p.tiles.find((t) => t.name === 'dropped')?.target), realpathSync.native(join(dir, 'dropped.txt')))
})

test('dropping a link adds a tile, fetches its title, and never navigates the window', async () => {
  const before = page.url()
  await drop('text/uri-list', fixtureUrl)
  await toast(/Added .* to Work/)
  await page.locator('.tile', { hasText: 'Local fixture page' }).waitFor({ timeout: 8000 })
  assert.equal(page.url(), before)
})

test('a link that redirects still gets its title (redirects are followed hop by hop)', async () => {
  await drop('text/uri-list', fixtureUrl.replace('/page', '/go'))
  await page.locator('.tile', { hasText: 'Redirect landed' }).waitFor({ timeout: 8000 })
})

test('dropping plain text is refused with a clear message', async () => {
  await drop('text/plain', 'hello world')
  await toast(/doesn't look like a link, app, file or folder/)
})

test('dropping the same link twice is refused', async () => {
  await drop('text/uri-list', fixtureUrl)
  await toast(/already here/)
})

test('Ctrl+V with a copied folder path adds a folder tile', async () => {
  clipboard.write(String.raw`C:\Windows\System32\drivers`)
  await page.locator('body').click({ position: { x: 5, y: 500 } }).catch(() => {})
  await page.keyboard.press('Control+V')
  await toast(/Added drivers/)
  assert.equal(await page.locator('.tile', { hasText: 'drivers' }).count(), 1)
})

test('palette: "calc" + Enter opens Calculator', async () => {
  const n = launches().length
  await page.locator('.search input').fill('calc')
  await page.locator('.search input').press('Enter')
  await page.waitForFunction(() => true)
  await new Promise((r) => setTimeout(r, 600))
  const log = launches().slice(n)
  assert.equal(log.at(-1)?.tileId, 't-calc')
})

test('pick an app from the Start menu list', async () => {
  await page.locator('nav').getByText('+ Add').click()
  await page.getByPlaceholder('Search apps').fill('Paint')
  await page.locator('.app-list [role="listitem"]', { hasText: /^Paint/ }).first().click({ timeout: 20000 })
  await toast(/Added Paint/)
  await page.keyboard.press('Escape')
})

test('right-click → Add to Start my day, then Start my day runs every step', async () => {
  await page.locator('nav').getByText('Work').click()
  await page.locator('.tile', { hasText: 'Weekly reports' }).click({ button: 'right' })
  await page.getByRole('menuitem', { name: /Add to Start my day/ }).click()
  const n = launches().length
  await page.locator('#start-my-day').click()
  await page.getByText('All set').waitFor({ timeout: 15000 })
  const ids = launches().slice(n).map((l) => l.tileId)
  assert.deepEqual(ids, ['t-notepad', 't-calc', 't-board', 't-reports'])
})

test('`bridge open wiki` from PowerShell opens the tile through the pipe', async () => {
  const n = launches().length
  const ps1 = join(home, 'bridge-functions.ps1')
  const out = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `. '${ps1}'; bridge open wiki`], { encoding: 'utf8' })
  assert.match(out, /Opened Team wiki/)
  assert.equal(launches().slice(n).at(-1)?.tileId, 't-wiki')
})

test('Delete then Undo puts the tile back in Start my day at the same spot', async () => {
  await page.locator('nav').getByText('Work').click()
  await page.locator('.tile', { hasText: 'Team board' }).click({ button: 'right' })
  await page.getByRole('menuitem', { name: /^Delete/ }).click()
  await page.locator('.toast', { hasText: 'Deleted Team board' }).getByRole('button', { name: 'Undo' }).click()
  await page.locator('.tile', { hasText: 'Team board' }).waitFor()
  await new Promise((r) => setTimeout(r, 300))
  const p = JSON.parse(readFileSync(join(home, 'profile.json'), 'utf8'))
  assert.deepEqual(p.routines[0].steps.map((s) => s.tileId), ['t-notepad', 't-calc', 't-board', 't-reports'])
})

test('`bridge add notes.txt` inside a folder adds that file, not a web link', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'bridge-cwd-'))
  writeFileSync(join(dir, 'notes.txt'), 'hi')
  const ps1 = join(home, 'bridge-functions.ps1')
  const out = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `Set-Location '${dir}'; . '${ps1}'; bridge add notes.txt`], { encoding: 'utf8' })
  assert.match(out, /Added notes/)
  const p = JSON.parse(readFileSync(join(home, 'profile.json'), 'utf8'))
  const t = p.tiles.find((x) => x.name === 'notes')
  assert.equal(t?.kind, 'file')
  assert.equal(realpathSync.native(t?.target), realpathSync.native(join(dir, 'notes.txt')))
  rmSync(dir, { recursive: true, force: true })
})

test('everything survives a restart', async () => {
  await app.close()
  await launch()
  await page.locator('nav').getByText('Work').click()
  assert.equal(await page.locator('.tile', { hasText: 'Local fixture page' }).count(), 1)
  const p = JSON.parse(readFileSync(join(home, 'profile.json'), 'utf8'))
  assert.equal(p.routines[0].steps.length, 4)
})

test('a damaged profile still opens and the user is told', async () => {
  await app.close()
  writeFileSync(join(home, 'profile.json'), '{ this is not json')
  await launch()
  await toast(/damaged/)
  assert.ok((await page.locator('.tile').count()) >= 4)
})
