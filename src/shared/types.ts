// Shared data contracts between main, preload and renderer. Change with care: every layer depends on these.

export type TileKind = 'url' | 'app' | 'folder' | 'file'

export interface Tile {
  id: string
  groupId: string
  name: string
  kind: TileKind
  /** URL, absolute path, `shell:AppsFolder\<AppID>`, or protocol URI (e.g. `ms-teams:`). */
  target: string
  /** Optional extra args for .exe targets. */
  args?: string[]
  /** Short word for the palette / CLI (`bridge open <keyword>`). */
  keyword?: string
  /** Second line on the card (hostname, "App", folder path tail…). */
  subtitle?: string
  /** File name inside `<BRIDGE_HOME>/icons`, served at `http://bridge-icon.localhost/<icon>`. */
  icon?: string
  createdAt: string
  lastOpenedAt?: string
  openCount: number
}

export interface Group {
  id: string
  name: string
}

/** Where a step's window goes: a monitor (stable display id) and a zone (see ZONES). */
export interface Placement {
  displayId: number
  zone: Zone
}

/** How to recognise a step's window: process file name (lowercase), optional title fragment, nth window of that app. */
export interface WindowHint {
  exe: string
  title?: string
  nth?: number
}

/** A routine/workspace step. No tileId = a window-only step (arrange an app that's already open). */
export interface RoutineStep {
  tileId?: string
  place?: Placement
  hint?: WindowHint
}

export const ZONES = [
  'full', 'left', 'right', 'top', 'bottom',
  'left-third', 'center-third', 'right-third', 'left-two-thirds', 'right-two-thirds',
  'top-third', 'middle-third', 'bottom-third',
  'top-left', 'top-right', 'bottom-left', 'bottom-right',
] as const
export type Zone = (typeof ZONES)[number]

/** A window found by "Capture current layout". */
export interface Captured {
  step: RoutineStep
  title: string
}

export interface Routine {
  id: string
  name: string
  steps: RoutineStep[]
}

/** The built-in routine behind the big button. Always present in a valid profile. */
export const START_MY_DAY_ID = 'r-start'

export type DisplayMode = 'auto' | 'pinned' | 'summon'

export interface Settings {
  hotkey: string
  mode: DisplayMode
  /** Stable display id remembered for pinned mode; null = choose automatically. */
  pinnedDisplayId: number | null
  calmMotion: boolean
  theme: ThemeName
  /** Optional accent override, #rrggbb (null = use the theme's). */
  accent?: string | null
  /** Glass strength 0–100. */
  glass: number
  density: 'comfortable' | 'compact'
  /** UI text size in percent (85–130). */
  fontScale: number
  /** Reactor frame cap while focused. */
  fps: 15 | 24 | 30 | 60
  quality: 'low' | 'medium' | 'high'
  /** Side widgets, in order. */
  widgets: WidgetName[]
}

export type ThemeName = 'command' | 'ember' | 'mono' | 'contrast'
export type WidgetName = 'routine' | 'recent' | 'pc' | 'focus' | 'notes' | 'clock'
export const DEFAULT_WIDGETS: WidgetName[] = ['routine', 'recent', 'pc']

export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

export interface Profile {
  version: 1
  groups: Group[]
  tiles: Tile[]
  routines: Routine[]
  settings: Settings
}

export interface Result<T = undefined> {
  ok: boolean
  /** Plain-language message, safe to show the user as-is. */
  message: string
  data?: T
}

export interface AddRequest {
  /** URL, path (quotes allowed), `shell:AppsFolder\…` or protocol URI. */
  input: string
  /** Name override (app picker passes the Start-menu name). */
  name?: string
  groupId: string
  /** Detect only; do not save. `data` is a Tile-shaped preview with id ''. */
  dryRun?: boolean
}

export interface AppEntry {
  name: string
  appId: string
}

export interface DisplayInfo {
  id: number
  /** "Screen 2 · 1920×1080" */
  label: string
  width: number
  height: number
  primary: boolean
  portrait: boolean
  /** Physical-pixel bounds and work area (for drawing the monitors to scale). */
  bounds: Rect
  work: Rect
}

export interface ShellState {
  mode: 'pinned' | 'summon'
  displayId: number
  /** 1-based screen number for labels ("Pinned to screen 2"). */
  screenNumber: number
  hotkey: string
  hotkeyOk: boolean
  /** The exact PowerShell profile line the user can add (shown, never auto-installed). */
  profileLine: string
}

export interface PcStats {
  /** 0–100 */
  cpu: number
  /** 0–100 */
  mem: number
}

export type RoutineStatus = 'running' | 'done' | 'failed' | 'finished'

export interface RoutineProgress {
  routineId: string
  /** 0-based step index; for 'finished' equals total. */
  step: number
  total: number
  status: RoutineStatus
  tileId?: string
  ok?: boolean
}

export type ToastTone = 'info' | 'ok' | 'error'

export type BridgeEvent =
  | { type: 'profile'; profile: Profile }
  | { type: 'shell'; state: ShellState }
  | { type: 'routine'; progress: RoutineProgress }
  | { type: 'launched'; tileId: string; ok: boolean; message: string }
  | { type: 'toast'; message: string; tone: ToastTone }
  /** Main asks the UI to do something (tray "Start my day", hotkey focus, etc.). */
  | { type: 'command'; command: 'focus-palette' | 'start-my-day' | 'open-settings' }
