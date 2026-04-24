import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { CssBaseline, ThemeProvider } from '@mui/material'
import { Toaster } from 'sonner'
import './index.css'
import App from './App.jsx'
import { appTheme } from './theme.js'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ThemeProvider theme={appTheme}>
      <CssBaseline />
      <App />
      <Toaster richColors position="bottom-right" />
    </ThemeProvider>
  </StrictMode>,
)
