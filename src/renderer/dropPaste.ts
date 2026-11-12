// Global drop + Ctrl+V → addItem into the active group. Also stops the webview from navigating to anything dropped.
import { useCallback, useEffect, useRef } from 'react'
import type { AddRequest } from '@shared/types'
import { useUi } from './state'

/** Drag type used when moving a tile onto a group. Global drop ignores drags that carry it. */
export const TILE_MIME = 'application/x-bridge-tile'

export const JUNK_MESSAGE = "That doesn't look like a link, app, file or folder"

/** The parts of a DataTransfer a drop reads (a real DataTransfer fits). */
export interface DropData {
  readonly files?: ArrayLike<File> | null
  getData(format: string): string
}

/**
 * Files first (their real paths), else uri-list lines (skipping `#` comments), else trimmed plain text.
 * Everything is read from `dt` synchronously, before the first await: the browser empties a DataTransfer once the drop
 * event returns. So call this from inside the drop handler, not later.
 */
export async function extractDropInputs(
  dt: DropData,
  pathsForFiles: (files: File[]) => Promise<string[]>,
): Promise<string[]> {
  const files = Array.from(dt.files ?? [])
  const uris = (dt.getData('text/uri-list') || '')
    .split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'))
  const text = (dt.getData('text/plain') || '').trim()
  if (files.length) {
    let paths: string[] = []
    try { paths = (await pathsForFiles(files)).filter(Boolean) } catch { /* not files on disk: fall back */ }
    if (paths.length) return paths
  }
  if (uris.length) return uris
  return text ? [text] : []
}

export function isEditable(el: Element | null): boolean {
  if (!el || !(el instanceof HTMLElement)) return false
  if (el.isContentEditable) return true
  const tag = el.tagName
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (tag === 'INPUT') {
    const type = (el as HTMLInputElement).type
    return !['button', 'checkbox', 'radio', 'range', 'submit', 'reset', 'color', 'file'].includes(type)
  }
  return false
}

/** Adds one item with the standard toast + reactor pulse. Returns true when it was saved. */
export function useAddItem(): (req: AddRequest) => Promise<boolean> {
  const { profile, toast, reactor } = useUi()
  const groupsRef = useRef(profile?.groups ?? [])
  groupsRef.current = profile?.groups ?? []
  return useCallback(async (req: AddRequest) => {
    const r = await window.bridge.addItem(req)
    if (r.ok && r.data) {
      const group = groupsRef.current.find((g) => g.id === (r.data!.groupId || req.groupId))
      toast(`Added ${r.data.name} to ${group?.name ?? 'Bridge'}`, 'ok')
      reactor.current?.pulse(12)
      return true
    }
    toast(r.message || JUNK_MESSAGE, 'error')
    return false
  }, [toast, reactor])
}

const hasTileDrag = (dt: DataTransfer | null) => !!dt && Array.from(dt.types).includes(TILE_MIME)
/** Text dragged onto a text field: let the field take it instead of adding a tile. */
const intoField = (e: DragEvent) =>
  isEditable(e.target as Element) && !Array.from(e.dataTransfer?.types ?? []).includes('Files')

export function useDropPaste(): void {
  const { profile, activeGroupId, dialog, toast } = useUi()
  const add = useAddItem()
  const live = useRef({ activeGroupId, groupName: '', dialogOpen: false, add, toast })
  live.current = {
    activeGroupId,
    groupName: profile?.groups.find((g) => g.id === activeGroupId)?.name ?? '',
    dialogOpen: dialog.kind !== 'none',
    add,
    toast,
  }

  useEffect(() => {
    const hint = document.createElement('div')
    hint.className = 'drop-hint'
    hint.setAttribute('aria-hidden', 'true')
    const label = document.createElement('span')
    hint.appendChild(label)
    document.body.appendChild(hint)
    let hideTimer = 0
    const hide = () => { clearTimeout(hideTimer); hint.classList.remove('on') }

    const addAll = async (inputs: string[]) => {
      const { activeGroupId: groupId, add: addItem, toast: say } = live.current
      if (!inputs.length) { say(JUNK_MESSAGE, 'error'); return }
      if (!groupId) { say('Make a group first, then add to it', 'error'); return }
      for (const input of inputs) await addItem({ input, groupId })
    }

    const onDragOver = (e: DragEvent) => {
      if (hasTileDrag(e.dataTransfer) || intoField(e)) return
      e.preventDefault()
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
      label.textContent = `Drop to add to ${live.current.groupName || 'Bridge'}`
      hint.classList.add('on')
      clearTimeout(hideTimer)
      hideTimer = window.setTimeout(hide, 180)
    }
    const onDrop = (e: DragEvent) => {
      if (intoField(e)) { hide(); return }
      e.preventDefault()
      hide()
      const dt = e.dataTransfer
      if (!dt || hasTileDrag(dt)) return
      // extractDropInputs reads dt synchronously here (before its first await), then waits for the real file paths.
      void extractDropInputs(dt, (files) => window.bridge.pathsForFiles(files)).then(addAll)
    }
    const onKeyDown = (e: KeyboardEvent) => {
      if (!(e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey && e.key.toLowerCase() === 'v')) return
      if (e.defaultPrevented || isEditable(document.activeElement) || live.current.dialogOpen) return
      e.preventDefault()
      void (async () => {
        const text = (await window.bridge.readClipboard()).trim()
        if (!text) { live.current.toast('Nothing to paste. Copy a link or a file first', 'error'); return }
        await addAll([text])
      })()
    }

    document.addEventListener('dragover', onDragOver)
    document.addEventListener('drop', onDrop)
    document.addEventListener('dragend', hide)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      hide()
      document.removeEventListener('dragover', onDragOver)
      document.removeEventListener('drop', onDrop)
      document.removeEventListener('dragend', hide)
      document.removeEventListener('keydown', onKeyDown)
      hint.remove()
    }
  }, [])
}
