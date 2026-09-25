import { createTheme } from '@mui/material/styles';

// System fonts only: the app must look right with no internet (no web-font downloads).
const systemFontStack = [
  '"Segoe UI Variable"',
  '"Segoe UI"',
  '-apple-system',
  'BlinkMacSystemFont',
  '"Helvetica Neue"',
  'Arial',
  'sans-serif',
].join(',');

export const theme = createTheme({
  palette: {
    mode: 'light',
    primary: { main: '#0f766e' },
    secondary: { main: '#475569' },
    background: { default: '#f8fafc', paper: '#ffffff' },
  },
  typography: {
    fontFamily: systemFontStack,
    fontSize: 15,
    button: { textTransform: 'none', fontWeight: 600 },
  },
  shape: { borderRadius: 8 },
  components: {
    MuiButton: { defaultProps: { size: 'large', disableElevation: true } },
  },
});
