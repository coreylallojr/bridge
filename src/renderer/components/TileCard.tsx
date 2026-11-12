// One tile: bracket-cornered glass card. Click / Enter / Space opens; right-click or Shift+F10 opens its menu.
import { useState, type CSSProperties } from 'react'
import type { Tile, TileKind } from '@shared/types'
import { useUi } from '../state'
import { TILE_MIME } from '../dropPaste'
import { iconUrl } from '../bridge'

export const KIND_LABEL: Record<TileKind, string> = { url: 'Link', app: 'App', folder: 'Folder', file: 'File' }

// Cyan, teal, sky, blue, amber: hues that sit inside the Command palette.
const HUES = [186, 172, 198, 212, 38]

export function tileHue(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0
  return HUES[h % HUES.length]
}

function monogram(name: string): string {
  const m = name.trim().match(/[\p{L}\p{N}]/u)
  return (m?.[0] ?? '?').toUpperCase()
}

interface Props {
  tile: Tile
  onMenu(tile: Tile, x: number, y: number): void
}

export function TileCard({ tile, onMenu }: Props) {
  const { openTile } = useUi()
  const [iconBroken, setIconBroken] = useState(false)
  const showIcon = !!tile.icon && !iconBroken
  const style = { '--hue': tileHue(tile.id) } as CSSProperties

  return (
    <div
      className="tile"
      role="button"
      tabIndex={0}
      draggable
      data-tile-id={tile.id}
      aria-label={`${tile.name}${tile.subtitle ? `, ${tile.subtitle}` : ''}`}
      title={tile.target}
      style={style}
      onClick={() => void openTile(tile)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); void openTile(tile) }
        if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
          e.preventDefault()
          const r = e.currentTarget.getBoundingClientRect()
          onMenu(tile, r.left + 24, r.top + 40)
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault()
        // Keyboard ContextMenu key reports 0,0; it is handled in onKeyDown.
        if (e.clientX === 0 && e.clientY === 0) return
        onMenu(tile, e.clientX, e.clientY)
      }}
      onDragStart={(e) => {
        e.dataTransfer.setData(TILE_MIME, tile.id)
        e.dataTransfer.effectAllowed = 'move'
        document.body.classList.add('dragging-tile')
      }}
      onDragEnd={() => document.body.classList.remove('dragging-tile')}
    >
      <div className={`shot ${showIcon ? 'has-icon' : 'mono'}`}>
        {showIcon && tile.icon
          ? <img src={iconUrl(tile.icon)} alt="" draggable={false} onError={() => setIconBroken(true)} />
          : <span className="letter" aria-hidden="true">{monogram(tile.name)}</span>}
        <em className="kind">{KIND_LABEL[tile.kind] ?? tile.kind}</em>
      </div>
      <span className="label">
        <b>{tile.name}</b>
        {tile.subtitle && <small>{tile.subtitle}</small>}
      </span>
      {tile.keyword && <span className="k">{tile.keyword}</span>}
    </div>
  )
}
