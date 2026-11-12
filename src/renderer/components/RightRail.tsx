// Right rail: the widgets chosen in Settings → Widgets, in their saved order.
import { DEFAULT_WIDGETS } from '@shared/types'
import { useUi } from '../state'
import { Widget } from './widgets'

export function RightRail() {
  const { profile } = useUi()
  const widgets = profile?.settings.widgets ?? DEFAULT_WIDGETS
  return (
    <aside>
      {widgets.length === 0 && <p className="empty">No widgets. Pick some in Settings.</p>}
      {widgets.map((w) => <Widget key={w} name={w} />)}
    </aside>
  )
}
