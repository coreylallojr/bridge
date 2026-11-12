// Left rail (a top strip in portrait): groups with tile counts, rename, delete, + New group, + Add.
import { useCallback, useEffect, useRef, useState } from 'react'
import { useUi } from '../state'
import { TILE_MIME } from '../dropPaste'
import { Menu } from './ContextMenu'

export const NEW_GROUP_EVENT = 'bridge:new-group'

export function GroupRail() {
  const { profile, activeGroupId, setActiveGroupId, setDialog, toast } = useUi()
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null)
  const [dropOn, setDropOn] = useState<string | null>(null)
  const pendingNew = useRef<string | null>(null)
  const groups = profile?.groups ?? []
  const groupsRef = useRef(groups)
  groupsRef.current = groups
  const counts = new Map<string, number>()
  for (const t of profile?.tiles ?? []) counts.set(t.groupId, (counts.get(t.groupId) ?? 0) + 1)

  const startRename = useCallback((id: string, name: string) => { setDraft(name); setEditing(id) }, [])

  // A new group shows up through the profile event; select it and start renaming once it is there.
  useEffect(() => {
    const id = pendingNew.current
    const g = id && profile?.groups.find((x) => x.id === id)
    if (!g) return
    pendingNew.current = null
    setActiveGroupId(g.id)
    startRename(g.id, g.name)
  }, [profile, setActiveGroupId, startRename])

  const newGroup = useCallback(async () => {
    const r = await window.bridge.addGroup('New group')
    if (!r.ok || !r.data) { toast(r.message, 'error'); return }
    if (groupsRef.current.some((g) => g.id === r.data!.id)) {
      setActiveGroupId(r.data.id)
      startRename(r.data.id, r.data.name)
    } else {
      pendingNew.current = r.data.id // selected by the effect above once the profile event lands
    }
  }, [toast, setActiveGroupId, startRename])

  useEffect(() => {
    const on = () => void newGroup()
    window.addEventListener(NEW_GROUP_EVENT, on)
    return () => window.removeEventListener(NEW_GROUP_EVENT, on)
  }, [newGroup])

  const commitRename = async () => {
    const id = editing
    if (!id) return
    setEditing(null)
    const name = draft.trim()
    const old = groups.find((g) => g.id === id)?.name
    if (!name || name === old) return
    const r = await window.bridge.renameGroup(id, name)
    if (!r.ok) toast(r.message, 'error')
  }

  const removeGroup = async (id: string) => {
    const g = groups.find((x) => x.id === id)
    if (!g) return
    if ((counts.get(id) ?? 0) > 0) { toast('Move or delete its tiles first', 'error'); return }
    const r = await window.bridge.deleteGroup(id)
    toast(r.ok ? `Deleted ${g.name}` : r.message, r.ok ? 'ok' : 'error')
  }

  const moveTileHere = async (tileId: string, groupId: string) => {
    const tile = profile?.tiles.find((t) => t.id === tileId)
    const g = groups.find((x) => x.id === groupId)
    if (!tile || !g || tile.groupId === groupId) return
    const r = await window.bridge.updateTile(tileId, { groupId })
    toast(r.ok ? `Moved ${tile.name} to ${g.name}` : r.message, r.ok ? 'ok' : 'error')
  }

  return (
    <nav aria-label="Groups">
      <div className="groups">
        {groups.map((g) =>
          editing === g.id ? (
            <input
              key={g.id}
              className="rename"
              aria-label="Group name"
              value={draft}
              maxLength={40}
              autoFocus
              onFocus={(e) => e.currentTarget.select()}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={() => void commitRename()}
              onKeyDown={(e) => {
                if (e.key === 'Enter') { e.preventDefault(); void commitRename() }
                if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setEditing(null) }
              }}
            />
          ) : (
            <button
              key={g.id}
              type="button"
              className={`g ${g.id === activeGroupId ? 'on' : ''} ${dropOn === g.id ? 'drop' : ''}`}
              aria-current={g.id === activeGroupId ? 'true' : undefined}
              title="Double-click to rename"
              onClick={() => setActiveGroupId(g.id)}
              onDoubleClick={() => startRename(g.id, g.name)}
              onContextMenu={(e) => {
                e.preventDefault()
                const r = e.currentTarget.getBoundingClientRect()
                const kb = e.clientX === 0 && e.clientY === 0
                setMenu({ id: g.id, x: kb ? r.left + 20 : e.clientX, y: kb ? r.bottom : e.clientY })
              }}
              onKeyDown={(e) => {
                if (e.key === 'F2') { e.preventDefault(); startRename(g.id, g.name) }
              }}
              onDragOver={(e) => {
                if (!Array.from(e.dataTransfer.types).includes(TILE_MIME)) return
                e.preventDefault()
                e.dataTransfer.dropEffect = 'move'
                if (dropOn !== g.id) setDropOn(g.id)
              }}
              onDragLeave={() => setDropOn((d) => (d === g.id ? null : d))}
              onDrop={(e) => {
                const id = e.dataTransfer.getData(TILE_MIME)
                setDropOn(null)
                if (!id) return
                e.preventDefault()
                void moveTileHere(id, g.id)
              }}
            >
              <span className="name">{g.name}</span>
              <span className="n">{counts.get(g.id) ?? 0}</span>
            </button>
          ),
        )}
      </div>
      <div className="foot">
        <button type="button" className="add" onClick={() => void newGroup()}>+ New group</button>
        <button type="button" className="add" onClick={() => setDialog({ kind: 'add' })}>+ Add</button>
      </div>
      {menu && (
        <Menu
          x={menu.x}
          y={menu.y}
          label="Group"
          onClose={() => setMenu(null)}
          items={[
            { label: 'Rename', run: () => startRename(menu.id, groups.find((g) => g.id === menu.id)?.name ?? '') },
            { label: 'Delete', danger: true, run: () => void removeGroup(menu.id) },
          ]}
        />
      )}
    </nav>
  )
}
