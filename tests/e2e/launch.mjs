// Launches the BUILT Bridge (Tauri release exe) and attaches Playwright over the WebView2 DevTools protocol.
// Used by the E2E suite and scripts/shot.mjs. Always pass a throwaway BRIDGE_HOME: each home gets its own
// single-instance mutex, CLI pipe and WebView2 data folder, so tests never touch the user's running Bridge.
import { chromium } from 'playwright-core'
import { spawn, execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { join, resolve } from 'node:path'

export const repo = resolve(import.meta.dirname, '..', '..')
export const exe = join(repo, 'src-tauri', 'target', 'release', 'bridge.exe')

const freePort = () =>
  new Promise((res, rej) => {
    const s = createServer()
    s.once('error', rej)
    s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => res(port)) })
  })

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/**
 * @param {{ home: string, size?: string, env?: Record<string,string>, args?: string[], cwd?: string }} o
 * @returns {Promise<{ page: import('playwright-core').Page, browser: import('playwright-core').Browser, pid: number, readyMs: () => number[], close: () => Promise<void> }>}
 */
export async function launchBridge({ home, size = '1600x900', env = {}, args = [], cwd = repo }) {
  if (!existsSync(exe)) throw new Error(`Build first (npm run tauri:build): ${exe} is missing`)
  const port = await freePort()
  const childEnv = {
    ...process.env,
    BRIDGE_HOME: home,
    BRIDGE_DRY_LAUNCH: '1',
    BRIDGE_TIMING: '1',
    ...(size ? { BRIDGE_WINDOW: size } : {}),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    ...env,
  }
  delete childEnv.ELECTRON_RUN_AS_NODE
  const proc = spawn(exe, args, { cwd, env: childEnv, stdio: 'ignore', windowsHide: false })
  let exited = false
  proc.once('exit', () => { exited = true })

  let browser
  const deadline = Date.now() + 20000
  while (!browser) {
    if (exited) throw new Error('Bridge exited during startup')
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 2000 })
    } catch (err) {
      if (Date.now() > deadline) { kill(proc.pid); throw err }
      await sleep(150)
    }
  }
  let page
  while (!page) {
    page = browser.contexts().flatMap((c) => c.pages()).find((p) => !p.url().startsWith('devtools:'))
    if (!page) { if (Date.now() > deadline) throw new Error('no Bridge page'); await sleep(100) }
  }
  await page.waitForSelector('.shell[data-ready="true"]', { timeout: 15000 })

  return {
    page,
    browser,
    pid: proc.pid,
    readyMs: () => {
      const log = join(home, 'bridge.log')
      if (!existsSync(log)) return []
      return [...readFileSync(log, 'utf8').matchAll(/ready after (\d+) ms/g)].map((m) => Number(m[1]))
    },
    close: async () => {
      await browser.close().catch(() => {})
      kill(proc.pid)
      for (let i = 0; i < 50 && !exited; i++) await sleep(100)
      await sleep(300) // let WebView2 release the data folder
    },
  }
}

function kill(pid) {
  try { execFileSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' }) } catch { /* already gone */ }
}

/** the user's clipboard is saved before and restored after the suite. */
export const clipboard = {
  read: () => execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Get-Clipboard -Raw'], { encoding: 'utf8' }).replace(/\r?\n$/, ''),
  write: (text) =>
    text
      ? execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Set-Clipboard -Value ([Console]::In.ReadToEnd())'], { input: text, encoding: 'utf8' })
      : execFileSync('cmd.exe', ['/d', '/c', 'clip < NUL'], { stdio: 'ignore' }),
}
