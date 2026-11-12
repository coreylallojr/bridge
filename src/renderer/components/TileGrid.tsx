// Tiles of the active group, plus the empty state and the tile right-click menu.
import { useState } from 'react'
import type { Tile } from '@shared/types'
import { useUi } from '../state'
import { TileCard } from './TileCard'
import { ContextMenu } from './ContextMenu'
import '../styles/tiles.css'

export function TileGrid() {
  const { profile, activeGroupId, setDialog } = useUi()
  const [menu, setMenu] = useState<{ tile: Tile; x: number; y: number } | null>(null)
  const tiles = profile?.tiles.filter((t) => t.groupId === activeGroupId) ?? []
  const group = profile?.groups.find((g) => g.id === activeGroupId)

  if (profile && tiles.length === 0) {
    return (
      <div className="tiles is-empty">
        <div className="empty-drop">
          <p>{group ? `${group.name} is empty.` : 'No groups yet.'} Drop a link, file or app here, or press <kbd>Ctrl</kbd>+<kbd>V</kbd>.</p>
          <button type="button" className="edit" onClick={() => setDialog({ kind: 'add' })}>+ Add</button>
        </div>
      </div>
    )
  }

  return (
    <div className="tiles" role="list" aria-label={group ? `${group.name} tiles` : 'Tiles'}>
      {tiles.map((t) => (
        <div role="listitem" className="tile-slot" key={t.id}>
          <TileCard tile={t} onMenu={(tile, x, y) => setMenu({ tile, x, y })} />
        </div>
      ))}
      {menu && <ContextMenu tile={menu.tile} x={menu.x} y={menu.y} onClose={() => setMenu(null)} />}
    </div>
  )
}
