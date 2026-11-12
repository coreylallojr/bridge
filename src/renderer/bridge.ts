// window.bridge on Tauri: every MainApi method is a Rust #[tauri::command] (snake_case name); events arrive on
// `bridge:event`. Uses the API Tauri injects (`withGlobalTauri`), so no npm package and no network.
import { API_METHODS, type BridgeApi, type MainApi } from '@shared/ipc'
import type { BridgeEvent } from '@shared/types'

interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> }
  event: { listen<T>(event: string, cb: (e: { payload: T }) => void): Promise<() => void> }
}

interface WebView2Host {
  postMessageWithAdditionalObjects(message: string, objects: ArrayLike<File>): void
}

declare global {
  interface Window {
    __TAURI__?: TauriGlobal
    chrome?: { webview?: WebView2Host }
  }
}

/** JS argument names per method (Tauri maps them to the Rust parameter names). */
const ARGS: { [K in keyof MainApi]: string[] } = {
  getProfile: [],
  getShellState: [],
  addItem: ['req'],
  updateTile: ['id', 'patch'],
  deleteTile: ['id'],
  restoreTile: ['tile'],
  addGroup: ['name'],
  renameGroup: ['id', 'name'],
  deleteGroup: ['id'],
  openTile: ['id'],
  setRoutineSteps: ['routineId', 'tileIds'],
  runRoutine: ['routineId'],
  listApps: [],
  readClipboard: [],
  updateSettings: ['patch'],
  listDisplays: [],
  getPcStats: [],
  hide: [],
  openDataFolder: [],
  copyText: ['text'],
  captureLayout: [],
  saveRoutine: ['routine'],
  deleteRoutine: ['id'],
}

export const snake = (s: string): string => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)

/** Tile icons are served by Rust's `bridge-icon` protocol (WebView2 form: http://bridge-icon.localhost/<file>). */
export const iconUrl = (file: string): string => `http://bridge-icon.localhost/${encodeURIComponent(file)}`

export function createBridge(tauri: TauriGlobal, host: WebView2Host | undefined): BridgeApi {
  const api: Record<string, unknown> = {}
  for (const m of API_METHODS) {
    const names = ARGS[m]
    api[m] = (...args: unknown[]) => {
      const payload: Record<string, unknown> = {}
      names.forEach((n, i) => { payload[n] = args[i] })
      return tauri.core.invoke(snake(m), payload)
    }
  }

  let nextDrop = 1
  const waiting = new Map<number, (paths: string[]) => void>()
  void tauri.event.listen<{ id: number; paths: string[] }>('bridge:drop-paths', ({ payload }) => {
    waiting.get(payload.id)?.(payload.paths)
    waiting.delete(payload.id)
  })

  /** Real disk paths of dropped files. WebView2 hands the File objects to Rust, which reads their paths. */
  api.pathsForFiles = (files: File[]): Promise<string[]> => {
    if (!files.length || !host) return Promise.resolve([])
    const id = nextDrop++
    return new Promise<string[]>((resolve) => {
      const timer = setTimeout(() => { waiting.delete(id); resolve([]) }, 3000)
      waiting.set(id, (paths) => { clearTimeout(timer); resolve(paths) })
      try {
        host.postMessageWithAdditionalObjects(JSON.stringify({ bridgeDrop: id }), files)
      } catch {
        // Not files on disk (e.g. dragged out of a browser download bubble): fall back to uri-list/text.
        clearTimeout(timer)
        waiting.delete(id)
        resolve([])
      }
    })
  }

  api.onEvent = (cb: (e: BridgeEvent) => void) => {
    let stop: (() => void) | null = null
    let cancelled = false
    void tauri.event.listen<BridgeEvent>('bridge:event', ({ payload }) => cb(payload)).then((un) => {
      if (cancelled) un()
      else stop = un
    })
    return () => { cancelled = true; stop?.() }
  }

  return api as unknown as BridgeApi
}

export function installBridge(): void {
  if (!window.__TAURI__) throw new Error('Bridge must run inside its desktop app')
  window.bridge = createBridge(window.__TAURI__, window.chrome?.webview)
}
