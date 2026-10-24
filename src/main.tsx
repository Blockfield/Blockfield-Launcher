import { createRoot } from 'react-dom/client'
import App from './app/App.tsx'
import './styles/index.css'

const render = () => createRoot(document.getElementById('root')!).render(<App />)

if (import.meta.env.VITE_CAPTURE_FIXTURES === '1') {
  void import('./lib/fixtures').then(async ({ install, drive }) => {
    await install()
    render()
    await drive()
  })
} else {
  render()
}
