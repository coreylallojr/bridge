import { createRoot } from 'react-dom/client'
import { installBridge } from './bridge'
import './styles/tokens.css'
import './styles/app.css'
import './styles/screens.css'
import { App } from './App'
import { UiProvider } from './state'

installBridge()

createRoot(document.getElementById('root')!).render(
  <UiProvider>
    <App />
  </UiProvider>,
)
