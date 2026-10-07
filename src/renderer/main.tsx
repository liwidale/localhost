import { createRoot } from 'react-dom/client'
import { App } from './App'
import { platformClass } from './shared/hooks'
import './styles/tokens.css'
import './styles/app.css'

platformClass()
createRoot(document.getElementById('root')!).render(<App />)
