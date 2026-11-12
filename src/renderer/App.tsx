// Layout skeleton (coordinator-owned). Components fill their own areas; see docs/plans/2026-11-02-m2-app-shell.md.
import { useUi } from './state'
import { useDropPaste } from './dropPaste'
import { TopBar } from './components/TopBar'
import { GroupRail } from './components/GroupRail'
import { Palette } from './components/Palette'
import { Core } from './components/Core'
import { TileGrid } from './components/TileGrid'
import { RightRail } from './components/RightRail'
import { SettingsDrawer } from './components/SettingsDrawer'
import { AddDialog } from './components/AddDialog'
import { EditTileDialog } from './components/EditTileDialog'
import { Toasts } from './components/Toasts'
import { Workspaces } from './components/Workspaces'

export function App() {
  const { profile } = useUi()
  useDropPaste()
  return (
    <>
      <canvas id="gl" />
      <div className="tint" />
      <div className="shell" data-ready={profile ? 'true' : 'false'}>
        <TopBar />
        <GroupRail />
        <main>
          <Palette />
          <Core />
          <TileGrid />
        </main>
        <RightRail />
      </div>
      <SettingsDrawer />
      <AddDialog />
      <EditTileDialog />
      <Workspaces />
      <Toasts />
    </>
  )
}
