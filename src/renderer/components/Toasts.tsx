// Bottom-center toast stack.
import { useUi } from '../state'

export function Toasts() {
  const { toasts, dismissToast } = useUi()
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.tone}`}>
          <span className="msg">{t.message}</span>
          {t.action && (
            <button
              type="button"
              className="act"
              onClick={() => { t.action!.run(); dismissToast(t.id) }}
            >
              {t.action.label}
            </button>
          )}
          <button type="button" className="close" aria-label="Dismiss" onClick={() => dismissToast(t.id)}>×</button>
        </div>
      ))}
    </div>
  )
}
