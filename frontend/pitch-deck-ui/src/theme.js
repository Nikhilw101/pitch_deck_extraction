import { createTheme } from '@mui/material/styles'

export const appTheme = createTheme({
  palette: {
    mode: 'dark',
    background: {
      default: '#070b14',
      paper: '#101726',
    },
    primary: {
      main: '#5b6ef5',
    },
    success: {
      main: '#22c98a',
    },
    error: {
      main: '#f05c5c',
    },
  },
  shape: {
    borderRadius: 14,
  },
  typography: {
    fontFamily: 'Inter, system-ui, sans-serif',
    button: {
      textTransform: 'none',
      fontWeight: 600,
    },
  },
})
