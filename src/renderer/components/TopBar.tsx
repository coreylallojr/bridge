// Top strip: wordmark, mode, Calm motion, Settings. Also owns the global Esc → hide.
import { useEffect } from 'react'
import { useUi } from '../state'

export function TopBar() {
  const { shell, profile, dialog, setDialog } = useUi()
  const calm = !!profile?.settings.calmMotion
  const openSettings = () => setDialog({ kind: 'settings' })

  // Esc on the bare home screen hides Bridge. Dialogs, palette and menus preventDefault when they use Esc.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented || dialog.kind !== 'none') return
      void window.bridge.hide()
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  }, [dialog.kind])

  return (
    <header className="top">
      <span className="word" aria-label="Bridge">BRID<i>G</i>E</span>
      <span className="spacer" />
      {shell && shell.hotkeyOk === false && (
        <button className="chip warn" onClick={openSettings} title={`${shell.hotkey} is in use by another app`}>
          <span className="dot amber" />Hotkey unavailable
        </button>
      )}
      {shell && (
        <button className="chip mode" onClick={openSettings} title="Change where Bridge lives">
          {shell.mode === 'pinned'
            ? <><span className="dot" />Pinned to screen {shell.screenNumber}</>
            : <><span className="dot summon" />Summoned</>}
        </button>
      )}
      <button
        className={`chip${calm ? ' on' : ''}`}
        aria-pressed={calm}
        onClick={() => void window.bridge.updateSettings({ calmMotion: !calm })}
        title={calm ? 'Let the reactor move again' : 'Hold the reactor still'}
      >
        Calm motion
      </button>
      <button className="chip" onClick={() => setDialog({ kind: 'workspaces' })} title="Start my day and workspaces: where your windows go">Workspaces</button>
      <button className="chip" onClick={openSettings}>Settings</button>
    </header>
  )
}
