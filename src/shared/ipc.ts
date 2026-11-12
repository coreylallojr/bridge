import type {
  AddRequest,
  AppEntry,
  Captured,
  BridgeEvent,
  DisplayInfo,
  Group,
  PcStats,
  Profile,
  Result,
  Routine,
  Settings,
  ShellState,
  Tile,
} from './types'

export type TilePatch = Partial<Pick<Tile, 'name' | 'target' | 'keyword' | 'groupId' | 'subtitle'>>

/** Async operations implemented in main. `window.bridge` exposes these plus the sync helpers in `BridgeApi`. */
export interface MainApi {
  getProfile(): Promise<Profile>
  getShellState(): Promise<ShellState>
  addItem(req: AddRequest): Promise<Result<Tile>>
  updateTile(id: string, patch: TilePatch): Promise<Result<Tile>>
  deleteTile(id: string): Promise<Result<Tile>>
  /** Undo for deleteTile: puts the exact tile back. */
  restoreTile(tile: Tile): Promise<Result>
  addGroup(name: string): Promise<Result<Group>>
  renameGroup(id: string, name: string): Promise<Result>
  /** Refused while the group still has tiles. */
  deleteGroup(id: string): Promise<Result>
  openTile(id: string): Promise<Result>
  setRoutineSteps(routineId: string, tileIds: string[]): Promise<Result>
  runRoutine(routineId: string): Promise<Result>
  listApps(): Promise<AppEntry[]>
  /** Clipboard text, or the path of a file copied in Explorer. '' if nothing usable. */
  readClipboard(): Promise<string>
  updateSettings(patch: Partial<Settings>): Promise<Result<ShellState>>
  listDisplays(): Promise<DisplayInfo[]>
  getPcStats(): Promise<PcStats>
  /** Esc: summon hides; pinned drops behind other windows. */
  hide(): Promise<void>
  openDataFolder(): Promise<void>
  copyText(text: string): Promise<void>
  /** The app windows on screen now, as window-only steps placed where they are. */
  captureLayout(): Promise<Captured[]>
  /** Creates (empty id) or replaces a routine/workspace. */
  saveRoutine(routine: Routine): Promise<Result<Routine>>
  deleteRoutine(id: string): Promise<Result>
}

export interface BridgeApi extends MainApi {
  /** Real filesystem paths of dropped Files ([] when they aren't files on disk). */
  pathsForFiles(files: File[]): Promise<string[]>
  onEvent(cb: (e: BridgeEvent) => void): () => void
}

/** IPC channel per MainApi method: `bridge:<method>`. Events go main → renderer on EVENT_CHANNEL. */
export const API_METHODS = [
  'getProfile',
  'getShellState',
  'addItem',
  'updateTile',
  'deleteTile',
  'restoreTile',
  'addGroup',
  'renameGroup',
  'deleteGroup',
  'openTile',
  'setRoutineSteps',
  'runRoutine',
  'listApps',
  'readClipboard',
  'updateSettings',
  'listDisplays',
  'getPcStats',
  'hide',
  'openDataFolder',
  'copyText',
  'captureLayout',
  'saveRoutine',
  'deleteRoutine',
] as const satisfies readonly (keyof MainApi)[]

export const channel = (m: keyof MainApi): string => `bridge:${m}`
export const EVENT_CHANNEL = 'bridge:event'

declare global {
  interface Window {
    bridge: BridgeApi
  }
}

// Compile-time guard: every MainApi method must be listed in API_METHODS.
type _Missing = Exclude<keyof MainApi, (typeof API_METHODS)[number]>
const _allListed: [_Missing] extends [never] ? true : _Missing = true
void _allListed
