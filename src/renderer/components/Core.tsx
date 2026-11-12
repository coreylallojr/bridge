// Center core: the reactor (on the full-window #gl canvas, centered on this box), the clock at its exact center,
// the date + status caption below it, and the START MY DAY button.
import { useEffect, useRef, useState } from 'react'
import { START_MY_DAY_ID } from '@shared/types'
import { useUi } from '../state'
import { Reactor } from '../reactor/Reactor'

const timeText = (d: Date) => `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, '0')}`
const dateText = (d: Date) =>
  d.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric' }).toUpperCase()

/** h:mm + date strings. Ticks every second but only re-renders when the minute changes. */
function useClock(): { time: string; date: string } {
  const [now, setNow] = useState(() => ({ time: timeText(new Date()), date: dateText(new Date()) }))
  useEffect(() => {
    const tick = () => {
      const d = new Date()
      const time = timeText(d)
      setNow((p) => (p.time === time ? p : { time, date: dateText(d) }))
    }
    const id = setInterval(tick, 1000)
    return () => clearInterval(id)
  }, [])
  return now
}

export function Core() {
  const { reactor, startMyDay, routine, profile } = useUi()
  const box = useRef<HTMLDivElement>(null)
  const { time, date } = useClock()
  const [after, setAfter] = useState<'' | 'ok' | 'bad'>('')
  const wasRunning = useRef(false)

  useEffect(() => {
    const r = new Reactor(document.getElementById('gl') as HTMLCanvasElement)
    reactor.current = r
    if (box.current) r.fitTo(box.current)
    return () => { r.dispose(); reactor.current = null }
  }, [reactor])

  // "All set" for a moment after a run finishes.
  useEffect(() => {
    if (routine.running) { wasRunning.current = true; setAfter(''); return }
    if (!wasRunning.current) return
    wasRunning.current = false
    if (routine.ok === undefined) return // cancelled / empty run: say nothing
    setAfter(routine.ok ? 'ok' : 'bad')
    const id = setTimeout(() => setAfter(''), 2600)
    return () => clearTimeout(id)
  }, [routine.running]) // outcome is read at the moment running flips off; no need to re-run on step updates

  const steps = profile?.routines.find((r) => r.id === START_MY_DAY_ID)?.steps
  const status = routine.running ? 'Starting your day'
    : after === 'ok' ? 'All set'
    : after === 'bad' ? 'Done, but something did not open'
    : ''

  return (
    <div className="core" ref={box}>
      <div className="readout" aria-label={`Time ${time}`}>
        <span className="time">{time}</span>
      </div>
      <div className="below">
        <div className="caption">
          <span className="date">{date}</span>
          <span className={`status${after === 'bad' ? ' warn' : ''}`} role="status">{status}</span>
        </div>
        <div className="launch">
          <button id="start-my-day" className="go" onClick={() => void startMyDay()} disabled={routine.running}>
            START MY DAY
          </button>
          {steps && steps.length === 0 && (
            <p className="hint">Right-click a tile, then choose Add to Start my day</p>
          )}
        </div>
      </div>
    </div>
  )
}
